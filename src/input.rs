use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{BTreeMap, HashMap};

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("open", "Open file", "Ctrl+O"),
    ("save", "Save", "Ctrl+S"),
    ("save_as", "Save As", "Ctrl+Shift+S"),
    ("close", "Close document", "Ctrl+W"),
    ("quit", "Quit Stet", "Ctrl+Q"),
    ("new", "New document", "Ctrl+N"),
    ("next", "Next document", "Ctrl+Tab"),
    ("previous", "Previous document", "Ctrl+Shift+Tab"),
    ("find", "Find", "Ctrl+F"),
    ("replace", "Find and replace", "Ctrl+H"),
    ("goto", "Go to line", "Ctrl+G"),
    ("undo", "Undo", "Ctrl+Z"),
    ("redo", "Redo", "Ctrl+Y"),
    ("select_all", "Select all", "Ctrl+A"),
    ("copy", "Copy", "Ctrl+C"),
    ("cut", "Cut", "Ctrl+X"),
    ("paste", "Paste", "Ctrl+V"),
    ("quick", "Quick open in current directory", "Ctrl+P"),
    ("palette", "Command palette", "Ctrl+Shift+P"),
    ("sidebar", "Toggle sidebar", "Ctrl+B"),
    ("split_vertical", "Split vertically", "Ctrl+\\"),
    ("split_horizontal", "Split horizontally", "Ctrl+Shift+\\"),
    ("close_pane", "Close pane", "Alt+W"),
    ("single_pane", "Single pane", "Alt+1"),
    ("pane_left", "Focus pane left", "Alt+Left"),
    ("pane_right", "Focus pane right", "Alt+Right"),
    ("pane_up", "Focus pane above", "Alt+Up"),
    ("pane_down", "Focus pane below", "Alt+Down"),
    ("help", "Keyboard shortcuts", "F1"),
    ("theme", "Cycle theme", "F6"),
    ("wrap", "Toggle line wrapping", "Alt+Z"),
    ("focus_sidebar", "Focus open files", "F8"),
    ("next_match", "Next search match", "F3"),
    ("previous_match", "Previous search match", "Shift+F3"),
    ("case", "Toggle search case sensitivity", "Alt+C"),
    ("language", "Set syntax language", "F7"),
    ("path", "Show full file path", "Alt+I"),
    ("reload", "Reload file from disk", "Alt+R"),
];

pub struct Bindings {
    map: HashMap<(KeyCode, KeyModifiers), String>,
}

impl Bindings {
    pub fn new(custom: BTreeMap<String, String>) -> Result<Self> {
        if custom
            .keys()
            .any(|key| !COMMANDS.iter().any(|(name, _, _)| name == key))
        {
            bail!("Unknown command in keybindings");
        }
        let mut map = HashMap::new();
        for (name, _, default) in COMMANDS {
            let binding = custom.get(*name).map(String::as_str).unwrap_or(default);
            if binding.is_empty() {
                continue;
            }
            let key = parse(binding)?;
            if let Some(conflict) = map.insert(key, (*name).to_owned()) {
                bail!("Binding {binding} conflicts between {conflict} and {name}");
            }
        }
        Ok(Self { map })
    }

    pub fn command(&self, key: KeyEvent) -> Option<&str> {
        let mut code = key.code;
        let mut modifiers = key.modifiers;
        if let KeyCode::Char(c) = code {
            code = KeyCode::Char(c.to_ascii_lowercase());
            if c == '|' && modifiers.contains(KeyModifiers::CONTROL) {
                code = KeyCode::Char('\\');
                modifiers.insert(KeyModifiers::SHIFT);
            }
        }
        if code == KeyCode::BackTab {
            code = KeyCode::Tab;
            modifiers.insert(KeyModifiers::SHIFT);
        }
        self.map.get(&(code, modifiers)).map(String::as_str)
    }
}

pub fn parse(value: &str) -> Result<(KeyCode, KeyModifiers)> {
    let parts: Vec<_> = value.split('+').collect();
    let mut modifiers = KeyModifiers::empty();
    for part in &parts[..parts.len().saturating_sub(1)] {
        let flag = match part.to_ascii_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "shift" => KeyModifiers::SHIFT,
            "alt" => KeyModifiers::ALT,
            _ => bail!("Invalid modifier in {value}"),
        };
        if modifiers.contains(flag) {
            bail!("Repeated modifier in {value}");
        }
        modifiers.insert(flag);
    }
    let last = parts.last().copied().unwrap_or("").to_ascii_lowercase();
    let code = match last.as_str() {
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "delete" => KeyCode::Delete,
        "backspace" => KeyCode::Backspace,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        _ if last.starts_with('f') && last.len() > 1 => {
            let n: u8 = last[1..]
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid function key: {value}"))?;
            if !(1..=24).contains(&n) {
                bail!("Function key must be F1 through F24");
            }
            KeyCode::F(n)
        }
        _ if last.chars().count() == 1 => KeyCode::Char(last.chars().next().unwrap_or(' ')),
        _ => bail!("Invalid key binding: {value}"),
    };
    if modifiers.is_empty()
        && matches!(
            code,
            KeyCode::Char(_)
                | KeyCode::Tab
                | KeyCode::Enter
                | KeyCode::Esc
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
        )
    {
        bail!("Editing keys cannot be bound without modifiers");
    }
    Ok((code, modifiers))
}

pub fn fuzzy(query: &str, value: &str) -> bool {
    let lowered = value.to_lowercase();
    let mut chars = lowered.chars();
    query.to_lowercase().chars().all(|c| chars.any(|v| v == c))
}
