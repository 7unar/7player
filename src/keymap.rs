//! Configurable keyboard bindings.
//!
//! The UI never looks at raw `KeyEvent`s outside this module. A key event is
//! normalized into a [`KeyChord`], resolved against the user's [`Keymap`], and
//! only then turned into an [`Action`]. This keeps terminal-specific key codes
//! away from application state and makes bindings testable without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// High-level commands that can be bound to keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    Help,
    Theme,
    Login,
    Logout,
    PlayPause,
    Next,
    Prev,
    NavPrev,
    NavNext,
    Sidebar,
    Queue,
    Lyrics,
    Search,
    Shuffle,
    Repeat,
    Mute,
    Like,
    Enqueue,
    EnqueueAll,
    Refresh,
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    FastPageUp,
    FastPageDown,
    HalfPageUp,
    HalfPageDown,
    Top,
    Bottom,
    PrevTab,
    NextTab,
    ScreenHome,
    ScreenSearch,
    ScreenLibrary,
    ScreenQueue,
    Activate,
    Back,
    SeekBack,
    SeekForward,
    RemoveQueue,
    VolumeUp,
    VolumeDown,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::Quit,
        Action::Help,
        Action::Theme,
        Action::Login,
        Action::Logout,
        Action::PlayPause,
        Action::Next,
        Action::Prev,
        Action::NavPrev,
        Action::NavNext,
        Action::Sidebar,
        Action::Queue,
        Action::Lyrics,
        Action::Search,
        Action::Shuffle,
        Action::Repeat,
        Action::Mute,
        Action::Like,
        Action::Enqueue,
        Action::EnqueueAll,
        Action::Refresh,
        Action::MoveUp,
        Action::MoveDown,
        Action::PageUp,
        Action::PageDown,
        Action::FastPageUp,
        Action::FastPageDown,
        Action::HalfPageUp,
        Action::HalfPageDown,
        Action::Top,
        Action::Bottom,
        Action::PrevTab,
        Action::NextTab,
        Action::ScreenHome,
        Action::ScreenSearch,
        Action::ScreenLibrary,
        Action::ScreenQueue,
        Action::Activate,
        Action::Back,
        Action::SeekBack,
        Action::SeekForward,
        Action::RemoveQueue,
        Action::VolumeUp,
        Action::VolumeDown,
    ];

    /// Stable name used in `config.toml` and documentation.
    pub fn id(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Help => "help",
            Action::Theme => "theme",
            Action::Login => "login",
            Action::Logout => "logout",
            Action::PlayPause => "play_pause",
            Action::Next => "next",
            Action::Prev => "prev",
            Action::NavPrev => "nav_prev",
            Action::NavNext => "nav_next",
            Action::Sidebar => "sidebar",
            Action::Queue => "queue",
            Action::Lyrics => "lyrics",
            Action::Search => "search",
            Action::Shuffle => "shuffle",
            Action::Repeat => "repeat",
            Action::Mute => "mute",
            Action::Like => "like",
            Action::Enqueue => "enqueue",
            Action::EnqueueAll => "enqueue_all",
            Action::Refresh => "refresh",
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::PageUp => "page_up",
            Action::PageDown => "page_down",
            Action::FastPageUp => "fast_page_up",
            Action::FastPageDown => "fast_page_down",
            Action::HalfPageUp => "half_page_up",
            Action::HalfPageDown => "half_page_down",
            Action::Top => "top",
            Action::Bottom => "bottom",
            Action::PrevTab => "prev_tab",
            Action::NextTab => "next_tab",
            Action::ScreenHome => "screen_home",
            Action::ScreenSearch => "screen_search",
            Action::ScreenLibrary => "screen_library",
            Action::ScreenQueue => "screen_queue",
            Action::Activate => "activate",
            Action::Back => "back",
            Action::SeekBack => "seek_back",
            Action::SeekForward => "seek_forward",
            Action::RemoveQueue => "remove_queue",
            Action::VolumeUp => "volume_up",
            Action::VolumeDown => "volume_down",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Action::ALL.iter().copied().find(|action| action.id() == id)
    }
}

/// One key press: a terminal key code plus the modifiers that matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    pub fn from_event(event: &KeyEvent) -> Self {
        let mut code = event.code;
        if code == KeyCode::Tab && event.modifiers.contains(KeyModifiers::SHIFT) {
            code = KeyCode::BackTab;
        }
        // Shift is already reflected in `KeyCode::Char('X')`; keeping it in
        // the modifier mask would make `L` impossible to bind. Ctrl/Alt are
        // real modifiers and are preserved.
        let mods = event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        Self { code, mods }
    }
}

/// A user-provided override: either one binding or a list of alternatives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingValue {
    One(String),
    Many(Vec<String>),
}

impl BindingValue {
    fn values(&self) -> Vec<String> {
        match self {
            BindingValue::One(value) => vec![value.clone()],
            BindingValue::Many(values) => values.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMatch {
    Action(Action),
    Pending,
    None,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(Vec<KeyChord>, Action)>,
}

const DEFAULT_BINDINGS: &[(Action, &[&str])] = &[
    (Action::Quit, &["q"]),
    (Action::Help, &["?"]),
    (Action::Theme, &["t"]),
    (Action::Login, &["L"]),
    (Action::Logout, &["ctrl+l"]),
    (Action::PlayPause, &["space"]),
    (Action::Next, &["n"]),
    (Action::Prev, &["p"]),
    (Action::NavPrev, &["h"]),
    (Action::NavNext, &["l"]),
    (Action::Sidebar, &["b"]),
    (Action::Queue, &["f"]),
    (Action::Lyrics, &["y"]),
    (Action::Search, &["/", "i"]),
    (Action::Shuffle, &["s"]),
    (Action::Repeat, &["r"]),
    (Action::Mute, &["m"]),
    (Action::Like, &["z"]),
    (Action::Enqueue, &["a"]),
    (Action::EnqueueAll, &["A"]),
    (Action::Refresh, &["R"]),
    (Action::MoveUp, &["up", "k"]),
    (Action::MoveDown, &["down", "j"]),
    (Action::PageUp, &["pgup"]),
    (Action::PageDown, &["pgdown"]),
    (Action::FastPageUp, &["ctrl+b"]),
    (Action::FastPageDown, &["ctrl+f"]),
    (Action::HalfPageUp, &["ctrl+u"]),
    (Action::HalfPageDown, &["ctrl+d"]),
    (Action::Top, &["g g", "home", "0"]),
    (Action::Bottom, &["end", "G", "$"]),
    (Action::PrevTab, &["["]),
    (Action::NextTab, &["]"]),
    (Action::ScreenHome, &["1"]),
    (Action::ScreenSearch, &["2"]),
    (Action::ScreenLibrary, &["3"]),
    (Action::ScreenQueue, &["4"]),
    (Action::Activate, &["enter"]),
    (Action::Back, &["esc"]),
    (Action::SeekBack, &["left"]),
    (Action::SeekForward, &["right"]),
    (Action::RemoveQueue, &["d"]),
    (Action::VolumeUp, &["+", "="]),
    (Action::VolumeDown, &["-", "_"]),
];

impl Default for Keymap {
    fn default() -> Self {
        Self::from_config(&BTreeMap::new()).0
    }
}

impl Keymap {
    /// Build the effective keymap: built-in defaults plus per-action overrides.
    ///
    /// Unknown action names and malformed bindings are returned as warnings and
    /// leave the default for that action untouched. An empty array or the
    /// string `"none"` disables an action entirely.
    pub fn from_config(overrides: &BTreeMap<String, BindingValue>) -> (Self, Vec<String>) {
        let mut bindings = Vec::new();
        for (action, keys) in DEFAULT_BINDINGS {
            for raw in *keys {
                let sequence = parse_sequence(raw).expect("built-in keybinding must be valid");
                bindings.push((sequence, *action));
            }
        }

        let mut warnings = Vec::new();
        for (id, value) in overrides {
            let Some(action) = Action::from_id(id) else {
                warnings.push(format!("未知按键动作 `{id}`，已忽略"));
                continue;
            };
            let raw_values = value.values();
            let mut parsed = Vec::new();
            let mut disabled = raw_values.is_empty();
            for raw in &raw_values {
                if raw.trim().eq_ignore_ascii_case("none") {
                    disabled = true;
                    break;
                }
                match parse_sequence(raw) {
                    Ok(sequence) => parsed.push(sequence),
                    Err(err) => warnings.push(format!(
                        "动作 `{id}` 的按键 `{raw}` 无效（{err}），已保留默认绑定"
                    )),
                }
            }
            if disabled {
                bindings.retain(|(_, bound)| *bound != action);
            } else if parsed.is_empty() {
                // Every requested binding failed validation; keep defaults.
                continue;
            } else {
                bindings.retain(|(_, bound)| *bound != action);
                for sequence in parsed {
                    if let Some((old_keys, old_action)) =
                        bindings.iter().find(|(keys, _)| *keys == sequence).cloned()
                    {
                        warnings.push(format!(
                            "按键 {} 同时绑定到 `{}` 和 `{}`，后者覆盖前者",
                            format_sequence(&old_keys),
                            old_action.id(),
                            action.id()
                        ));
                        bindings.retain(|(keys, _)| *keys != sequence);
                    }
                    bindings.push((sequence, action));
                }
            }
        }

        (Self { bindings }, warnings)
    }

    /// Resolve a chord, optionally continuing a multi-key sequence such as `g g`.
    pub fn resolve(&self, pending: &[KeyChord], chord: KeyChord) -> KeyMatch {
        let mut sequence = pending.to_vec();
        sequence.push(chord.clone());
        if let Some(action) = self.action_for(&sequence) {
            return KeyMatch::Action(action);
        }
        if self.is_prefix(&sequence) {
            return KeyMatch::Pending;
        }
        if !pending.is_empty() {
            // A pending prefix is not part of a valid sequence; fall back to
            // interpreting the new key as a standalone binding.
            let single = vec![chord];
            if let Some(action) = self.action_for(&single) {
                return KeyMatch::Action(action);
            }
            if self.is_prefix(&single) {
                return KeyMatch::Pending;
            }
        }
        KeyMatch::None
    }

    pub fn keys_for(&self, action: Action) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(keys, _)| format_sequence(keys))
            .collect()
    }

    pub fn key_label(&self, action: Action) -> String {
        self.keys_for(action).join(" / ")
    }

    /// First binding for an action, used by compact footer hints.
    pub fn primary_label(&self, action: Action) -> Option<String> {
        self.bindings
            .iter()
            .find(|(_, bound)| *bound == action)
            .map(|(keys, _)| format_sequence(keys))
    }

    fn action_for(&self, sequence: &[KeyChord]) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(keys, _)| keys == sequence)
            .map(|(_, action)| *action)
    }

    fn is_prefix(&self, sequence: &[KeyChord]) -> bool {
        self.bindings
            .iter()
            .any(|(keys, _)| keys.starts_with(sequence))
    }
}

fn parse_sequence(raw: &str) -> Result<Vec<KeyChord>, String> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.is_empty() {
        return Err("空按键".into());
    }
    tokens
        .iter()
        .map(|token| parse_chord(token))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_chord(raw: &str) -> Result<KeyChord, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("空按键".into());
    }
    if raw == "+" {
        return Ok(KeyChord {
            code: KeyCode::Char('+'),
            mods: KeyModifiers::NONE,
        });
    }
    if raw == "-" {
        return Ok(KeyChord {
            code: KeyCode::Char('-'),
            mods: KeyModifiers::NONE,
        });
    }

    let mut mods = KeyModifiers::NONE;
    let mut shift = false;
    let mut key = None;
    for part in raw.split('+') {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "alt" | "option" => mods |= KeyModifiers::ALT,
            "shift" => shift = true,
            "" => return Err("空按键片段".into()),
            _ => {
                if key.is_some() {
                    return Err("包含多个主键".into());
                }
                key = Some(part);
            }
        }
    }
    let key = key.ok_or_else(|| "缺少主键".to_string())?;
    let mut code = parse_key_code(key)?;
    if code == KeyCode::Tab && shift {
        code = KeyCode::BackTab;
    }
    Ok(KeyChord { code, mods })
}

fn parse_key_code(key: &str) -> Result<KeyCode, String> {
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "space" => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdown" | "pgdn" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "plus" => KeyCode::Char('+'),
        "minus" => KeyCode::Char('-'),
        "slash" => KeyCode::Char('/'),
        "question" => KeyCode::Char('?'),
        _ => {
            if let Some(number) = lower.strip_prefix('f') {
                if let Ok(number) = number.parse::<u8>() {
                    if (1..=24).contains(&number) {
                        return Ok(KeyCode::F(number));
                    }
                }
            }
            let mut chars = key.chars();
            let first = chars.next().ok_or_else(|| "空按键".to_string())?;
            if chars.next().is_some() {
                return Err(format!("未知按键 `{key}`"));
            }
            KeyCode::Char(first)
        }
    };
    Ok(code)
}

fn format_sequence(sequence: &[KeyChord]) -> String {
    sequence
        .iter()
        .map(format_chord)
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_chord(chord: &KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.mods.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl".to_string());
    }
    if chord.mods.contains(KeyModifiers::ALT) {
        parts.push("alt".to_string());
    }
    let code = match chord.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "backtab".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pgup".to_string(),
        KeyCode::PageDown => "pgdown".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::F(number) => format!("f{number}"),
        _ => "?".to_string(),
    };
    parts.push(code);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(raw: &str) -> KeyChord {
        parse_sequence(raw).unwrap()[0].clone()
    }

    #[test]
    fn built_in_gg_is_a_sequence() {
        let keymap = Keymap::default();
        assert_eq!(keymap.resolve(&[], chord("g")), KeyMatch::Pending);
        assert_eq!(
            keymap.resolve(&[chord("g")], chord("g")),
            KeyMatch::Action(Action::Top)
        );
    }

    #[test]
    fn modified_keys_are_distinct() {
        let keymap = Keymap::default();
        assert_eq!(
            keymap.resolve(&[], chord("ctrl+d")),
            KeyMatch::Action(Action::HalfPageDown)
        );
        assert_eq!(
            keymap.resolve(&[], chord("d")),
            KeyMatch::Action(Action::RemoveQueue)
        );
        assert_eq!(
            keymap.resolve(&[], chord("L")),
            KeyMatch::Action(Action::Login)
        );
        assert_eq!(
            keymap.resolve(&[], chord("ctrl+l")),
            KeyMatch::Action(Action::Logout)
        );
    }

    #[test]
    fn overrides_replace_only_the_selected_action() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "move_up".to_string(),
            BindingValue::Many(vec!["w".into(), "up".into()]),
        );
        let (keymap, warnings) = Keymap::from_config(&overrides);
        assert!(warnings.is_empty());
        assert_eq!(keymap.keys_for(Action::MoveUp), vec!["w", "up"]);
        assert_eq!(
            keymap.resolve(&[], chord("k")),
            KeyMatch::None,
            "k must be removed when move_up is overridden"
        );
        assert_eq!(
            keymap.resolve(&[], chord("w")),
            KeyMatch::Action(Action::MoveUp)
        );
        // Other defaults remain intact.
        assert_eq!(
            keymap.resolve(&[], chord("j")),
            KeyMatch::Action(Action::MoveDown)
        );
    }

    #[test]
    fn empty_binding_disables_action() {
        let mut overrides = BTreeMap::new();
        overrides.insert("help".to_string(), BindingValue::Many(Vec::new()));
        let (keymap, _) = Keymap::from_config(&overrides);
        assert_eq!(keymap.resolve(&[], chord("?")), KeyMatch::None);
    }

    #[test]
    fn invalid_action_and_binding_warn() {
        let mut overrides = BTreeMap::new();
        overrides.insert("not_an_action".to_string(), BindingValue::One("x".into()));
        overrides.insert("quit".to_string(), BindingValue::One("ctrl+".into()));
        let (keymap, warnings) = Keymap::from_config(&overrides);
        assert_eq!(warnings.len(), 2);
        // Invalid quit binding keeps the default.
        assert_eq!(
            keymap.resolve(&[], chord("q")),
            KeyMatch::Action(Action::Quit)
        );
    }

    #[test]
    fn shifted_character_normalizes_to_code() {
        let event = KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT);
        let normalized = KeyChord::from_event(&event);
        assert_eq!(normalized, chord("L"));
    }

    #[test]
    fn conflict_keeps_last_action() {
        let mut overrides = BTreeMap::new();
        overrides.insert("help".to_string(), BindingValue::One("q".into()));
        let (keymap, warnings) = Keymap::from_config(&overrides);
        assert!(!warnings.is_empty());
        assert_eq!(
            keymap.resolve(&[], chord("q")),
            KeyMatch::Action(Action::Help)
        );
    }

    #[test]
    fn special_keys_are_supported() {
        assert_eq!(chord("pgup").code, KeyCode::PageUp);
        assert_eq!(chord("space").code, KeyCode::Char(' '));
        assert_eq!(chord("f5").code, KeyCode::F(5));
        assert_eq!(chord("+").code, KeyCode::Char('+'));
        assert_eq!(chord("minus").code, KeyCode::Char('-'));
    }
}
