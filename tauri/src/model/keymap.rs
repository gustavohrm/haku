//! Browser shortcuts: which keys do what.
//!
//! There is one keymap, and it is read natively, whichever webview has focus,
//! so a shortcut behaves the same over a page as over the interface. Keys it
//! does not name reach the page or the interface untouched.
//!
//! [`DEFAULTS`] holds every action with its default chords. The user may
//! change any of them; [`Keymap`] is the defaults with those changes applied.
//! [Shortcuts § Changing bindings](../../../docs/specs/shortcuts.md#changing-bindings).

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::{LazyLock, PoisonError, RwLock};

use serde::{Deserialize, Serialize};
use specta::Type;

/// A key a shortcut can be bound to, independent of the platform's codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A letter key, as its uppercase character.
    Letter(char),
    /// A digit key on the main row, 0 to 9.
    Digit(u8),
    /// F1 to F24.
    Function(u8),
    Tab,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Escape,
    Home,
    End,
    Insert,
    Delete,
}

/// Named keys, as a chord is written.
const NAMED_KEYS: &[(Key, &str)] = &[
    (Key::Tab, "Tab"),
    (Key::Left, "Left"),
    (Key::Right, "Right"),
    (Key::Up, "Up"),
    (Key::Down, "Down"),
    (Key::PageUp, "PageUp"),
    (Key::PageDown, "PageDown"),
    (Key::Escape, "Esc"),
    (Key::Home, "Home"),
    (Key::End, "End"),
    (Key::Insert, "Insert"),
    (Key::Delete, "Delete"),
];

impl Key {
    fn parse(name: &str) -> Option<Self> {
        let mut chars = name.chars();
        if let (Some(only), None) = (chars.next(), chars.next()) {
            return match only {
                'a'..='z' | 'A'..='Z' => Some(Self::Letter(only.to_ascii_uppercase())),
                '0'..='9' => Some(Self::Digit(only as u8 - b'0')),
                _ => None,
            };
        }
        if let Some(number) = name.strip_prefix(['F', 'f']).and_then(|n| n.parse::<u8>().ok()) {
            return (1..=24).contains(&number).then_some(Self::Function(number));
        }
        NAMED_KEYS
            .iter()
            .find(|(_, spelled)| spelled.eq_ignore_ascii_case(name))
            .map(|(key, _)| *key)
    }

    /// Keys pages and text fields move and edit by.
    const fn edits(self) -> bool {
        matches!(
            self,
            Self::Insert | Self::Delete | Self::Home | Self::End | Self::Left | Self::Right | Self::Up | Self::Down
        )
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(letter) => write!(f, "{letter}"),
            Self::Digit(digit) => write!(f, "{digit}"),
            Self::Function(number) => write!(f, "F{number}"),
            named => {
                let (_, spelled) = NAMED_KEYS
                    .iter()
                    .find(|(key, _)| key == named)
                    .expect("every other key is named");
                f.write_str(spelled)
            }
        }
    }
}

/// A key pressed with the modifiers held at the time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Why a chord cannot be bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChordRefusal {
    /// It holds neither Ctrl nor Alt, so it would take a key the user types.
    NeedsModifier,
    /// Pages and text fields edit or move by it, or the system closes the
    /// window with it.
    Reserved,
}

impl Chord {
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    pub const fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::plain(key)
        }
    }

    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::ctrl(key)
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            alt: true,
            ..Self::plain(key)
        }
    }

    pub const fn alt_shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::alt(key)
        }
    }

    pub const fn shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::plain(key)
        }
    }

    /// Reads a chord as [`fmt::Display`] writes it, ignoring case and the
    /// order of the modifiers.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = Key::parse(parts.pop()?)?;
        let mut chord = Self::plain(key);
        for modifier in parts {
            let held = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" => &mut chord.ctrl,
                "alt" => &mut chord.alt,
                "shift" => &mut chord.shift,
                _ => return None,
            };
            *held = true;
        }
        Some(chord)
    }

    /// Why this chord cannot be bound, or `None` when it can.
    pub fn refusal(self) -> Option<ChordRefusal> {
        let Self { key, ctrl, shift, alt } = self;
        let function = matches!(key, Key::Function(_));
        if !(ctrl || alt || function || (shift && key == Key::Escape)) {
            return Some(ChordRefusal::NeedsModifier);
        }
        let editing_letter = matches!(key, Key::Letter('A' | 'C' | 'V' | 'X' | 'Y' | 'Z'));
        let reserved = (ctrl && editing_letter && !(shift && alt))
            || (ctrl && !alt && key.edits())
            || (alt && !ctrl && !shift && key == Key::Function(4));
        reserved.then_some(ChordRefusal::Reserved)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [(self.ctrl, "Ctrl+"), (self.alt, "Alt+"), (self.shift, "Shift+")] {
            if held {
                f.write_str(name)?;
            }
        }
        write!(f, "{}", self.key)
    }
}

/// What a shortcut asks the browser to do; dispatch validates its source tab.
///
/// Its camelCase name is the action's id in `Settings.shortcuts`, so a
/// variant is never renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Shortcut {
    NewTab,
    NewWindow,
    NewPrivateWindow,
    ToggleSplit,
    MoveTabLeft,
    MoveTabRight,
    CloseWindow,
    Home,
    Memory,
    DuplicateTab,
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    LastTab,
    FocusAddress,
    Reload,
    Back,
    Forward,
    DevTools,
    Find,
    Print,
    SavePage,
    ResetZoom,
}

impl Shortcut {
    /// The tab position this selects, counting from zero, for Tab1 to Tab8.
    pub const fn tab_index(self) -> Option<usize> {
        Some(match self {
            Self::Tab1 => 0,
            Self::Tab2 => 1,
            Self::Tab3 => 2,
            Self::Tab4 => 3,
            Self::Tab5 => 4,
            Self::Tab6 => 5,
            Self::Tab7 => 6,
            Self::Tab8 => 7,
            _ => return None,
        })
    }
}

/// Every action and its default chords, in the order settings lists them.
///
/// The defaults follow Chrome's, since that is what the hands already know.
pub const DEFAULTS: &[(Shortcut, &[Chord])] = {
    use Key::{Digit, Escape, Function, Home, Left, Letter, PageDown, PageUp, Right, Tab};
    &[
        (Shortcut::NewTab, &[Chord::ctrl(Letter('T'))]),
        (Shortcut::NewWindow, &[Chord::ctrl(Letter('N'))]),
        (Shortcut::NewPrivateWindow, &[Chord::ctrl_shift(Letter('N'))]),
        (
            Shortcut::CloseTab,
            &[Chord::ctrl(Letter('W')), Chord::ctrl(Function(4))],
        ),
        (Shortcut::ReopenClosedTab, &[Chord::ctrl_shift(Letter('T'))]),
        (Shortcut::NextTab, &[Chord::ctrl(Tab), Chord::ctrl(PageDown)]),
        (Shortcut::PreviousTab, &[Chord::ctrl_shift(Tab), Chord::ctrl(PageUp)]),
        (Shortcut::Tab1, &[Chord::ctrl(Digit(1))]),
        (Shortcut::Tab2, &[Chord::ctrl(Digit(2))]),
        (Shortcut::Tab3, &[Chord::ctrl(Digit(3))]),
        (Shortcut::Tab4, &[Chord::ctrl(Digit(4))]),
        (Shortcut::Tab5, &[Chord::ctrl(Digit(5))]),
        (Shortcut::Tab6, &[Chord::ctrl(Digit(6))]),
        (Shortcut::Tab7, &[Chord::ctrl(Digit(7))]),
        (Shortcut::Tab8, &[Chord::ctrl(Digit(8))]),
        (Shortcut::LastTab, &[Chord::ctrl(Digit(9))]),
        (
            Shortcut::FocusAddress,
            &[
                Chord::ctrl(Letter('L')),
                Chord::alt(Letter('D')),
                Chord::plain(Function(6)),
            ],
        ),
        (Shortcut::Reload, &[Chord::ctrl(Letter('R')), Chord::plain(Function(5))]),
        (Shortcut::Back, &[Chord::alt(Left)]),
        (Shortcut::Forward, &[Chord::alt(Right)]),
        (Shortcut::ToggleSplit, &[Chord::alt_shift(Letter('N'))]),
        (Shortcut::MoveTabLeft, &[Chord::ctrl_shift(PageUp)]),
        (Shortcut::MoveTabRight, &[Chord::ctrl_shift(PageDown)]),
        (Shortcut::CloseWindow, &[Chord::ctrl_shift(Letter('W'))]),
        (Shortcut::Home, &[Chord::alt(Home)]),
        (Shortcut::Memory, &[Chord::shift(Escape)]),
        (Shortcut::DuplicateTab, &[Chord::ctrl_shift(Letter('K'))]),
        (
            Shortcut::DevTools,
            &[Chord::plain(Function(12)), Chord::ctrl_shift(Letter('I'))],
        ),
        (Shortcut::Find, &[Chord::ctrl(Letter('F'))]),
        (Shortcut::Print, &[Chord::ctrl(Letter('P'))]),
        (Shortcut::SavePage, &[Chord::ctrl(Letter('S'))]),
        (Shortcut::ResetZoom, &[Chord::ctrl(Digit(0))]),
    ]
};

/// The default chords of `shortcut`.
pub fn defaults_of(shortcut: Shortcut) -> &'static [Chord] {
    DEFAULTS
        .iter()
        .find(|(action, _)| *action == shortcut)
        .map_or(&[], |(_, chords)| *chords)
}

/// Every action's chords: the defaults, with the user's changes applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Shortcut, Vec<Chord>)>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new(&BTreeMap::new())
    }
}

impl Keymap {
    /// The defaults, with `changed` replacing the chords of the actions it
    /// names. A chord the user gave an action is taken from any action that
    /// has it by default.
    pub fn new(changed: &BTreeMap<Shortcut, Vec<Chord>>) -> Self {
        let claimed: HashSet<Chord> = changed.values().flatten().copied().collect();
        let bindings = DEFAULTS
            .iter()
            .map(|(shortcut, defaults)| {
                let chords = changed.get(shortcut).cloned().unwrap_or_else(|| {
                    defaults
                        .iter()
                        .copied()
                        .filter(|chord| !claimed.contains(chord))
                        .collect()
                });
                (*shortcut, chords)
            })
            .collect();
        Self { bindings }
    }

    /// The shortcut a chord is bound to, if any. Modifiers must match
    /// exactly, so Ctrl+Shift+W is not Ctrl+W.
    pub fn shortcut_for(&self, chord: Chord) -> Option<Shortcut> {
        self.bindings
            .iter()
            .find(|(_, chords)| chords.contains(&chord))
            .map(|(shortcut, _)| *shortcut)
    }

    /// Every action with its chords, in [`DEFAULTS`] order.
    pub fn bindings(&self) -> impl Iterator<Item = (Shortcut, &[Chord])> {
        self.bindings
            .iter()
            .map(|(shortcut, chords)| (*shortcut, chords.as_slice()))
    }
}

/// The keymap the key handlers read. Set from the settings at startup and on
/// every change to them; held only to read or replace it, since the handlers
/// read it on the UI thread.
static ACTIVE: LazyLock<RwLock<Keymap>> = LazyLock::new(RwLock::default);

/// Makes `keymap` the one [`shortcut_for`] reads.
pub fn set_active(keymap: Keymap) {
    *ACTIVE.write().unwrap_or_else(PoisonError::into_inner) = keymap;
}

/// The keymap in effect.
pub fn active() -> Keymap {
    ACTIVE.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// The shortcut a chord is bound to in the keymap in effect, if any.
pub fn shortcut_for(chord: Chord) -> Option<Shortcut> {
    ACTIVE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .shortcut_for(chord)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(chord: Chord) -> Option<Shortcut> {
        Keymap::default().shortcut_for(chord)
    }

    #[test]
    fn ctrl_f_p_and_s_find_print_and_save_the_page() {
        assert_eq!(bound(Chord::ctrl(Key::Letter('F'))), Some(Shortcut::Find));
        assert_eq!(bound(Chord::ctrl(Key::Letter('P'))), Some(Shortcut::Print));
        assert_eq!(bound(Chord::ctrl(Key::Letter('S'))), Some(Shortcut::SavePage));
        assert_eq!(bound(Chord::ctrl_shift(Key::Letter('S'))), None);
    }

    #[test]
    fn ctrl_shift_n_opens_a_private_window() {
        assert_eq!(
            bound(Chord::ctrl_shift(Key::Letter('N'))),
            Some(Shortcut::NewPrivateWindow)
        );
        assert_eq!(bound(Chord::ctrl(Key::Letter('N'))), Some(Shortcut::NewWindow));
    }

    #[test]
    fn approved_chords_are_bound_with_exact_modifiers() {
        for (chord, expected) in [
            (Chord::alt_shift(Key::Letter('N')), Shortcut::ToggleSplit),
            (Chord::alt(Key::Home), Shortcut::Home),
            (Chord::shift(Key::Escape), Shortcut::Memory),
            (Chord::ctrl_shift(Key::PageUp), Shortcut::MoveTabLeft),
            (Chord::ctrl_shift(Key::PageDown), Shortcut::MoveTabRight),
            (Chord::ctrl_shift(Key::Letter('W')), Shortcut::CloseWindow),
            (Chord::ctrl_shift(Key::Letter('K')), Shortcut::DuplicateTab),
        ] {
            assert_eq!(bound(chord), Some(expected), "{chord:?}");
            assert_ne!(
                bound(chord),
                bound(Chord {
                    ctrl: !chord.ctrl,
                    ..chord
                })
            );
        }
    }

    #[test]
    fn tab_shortcuts_follow_chrome() {
        assert_eq!(bound(Chord::ctrl(Key::Letter('T'))), Some(Shortcut::NewTab));
        assert_eq!(bound(Chord::ctrl(Key::Letter('W'))), Some(Shortcut::CloseTab));
        assert_eq!(
            bound(Chord::ctrl_shift(Key::Letter('T'))),
            Some(Shortcut::ReopenClosedTab)
        );
        assert_eq!(bound(Chord::ctrl(Key::Tab)), Some(Shortcut::NextTab));
        assert_eq!(bound(Chord::ctrl_shift(Key::Tab)), Some(Shortcut::PreviousTab));
    }

    #[test]
    fn digits_select_by_position_and_nine_selects_the_last() {
        assert_eq!(bound(Chord::ctrl(Key::Digit(1))), Some(Shortcut::Tab1));
        assert_eq!(bound(Chord::ctrl(Key::Digit(8))), Some(Shortcut::Tab8));
        assert_eq!(Shortcut::Tab1.tab_index(), Some(0));
        assert_eq!(Shortcut::Tab8.tab_index(), Some(7));
        assert_eq!(Shortcut::LastTab.tab_index(), None);
        assert_eq!(bound(Chord::ctrl(Key::Digit(9))), Some(Shortcut::LastTab));
        assert_eq!(bound(Chord::ctrl(Key::Digit(0))), Some(Shortcut::ResetZoom));
    }

    #[test]
    fn alt_and_function_key_bindings_follow_chrome() {
        assert_eq!(bound(Chord::alt(Key::Letter('D'))), Some(Shortcut::FocusAddress));
        assert_eq!(bound(Chord::alt(Key::Left)), Some(Shortcut::Back));
        assert_eq!(bound(Chord::alt(Key::Right)), Some(Shortcut::Forward));
        assert_eq!(bound(Chord::ctrl(Key::Function(4))), Some(Shortcut::CloseTab));
        assert_eq!(bound(Chord::plain(Key::Function(5))), Some(Shortcut::Reload));
        assert_eq!(bound(Chord::plain(Key::Function(6))), Some(Shortcut::FocusAddress));
        assert_eq!(bound(Chord::plain(Key::Function(12))), Some(Shortcut::DevTools));
        assert_eq!(bound(Chord::ctrl_shift(Key::Letter('I'))), Some(Shortcut::DevTools));
        assert_eq!(bound(Chord::ctrl(Key::PageDown)), Some(Shortcut::NextTab));
        assert_eq!(bound(Chord::ctrl(Key::PageUp)), Some(Shortcut::PreviousTab));
    }

    #[test]
    fn modifiers_must_match_exactly() {
        assert_eq!(bound(Chord::alt(Key::Letter('N'))), None);
        assert_eq!(bound(Chord::plain(Key::Escape)), None);
        assert_eq!(bound(Chord::plain(Key::Letter('T'))), None);
        assert_eq!(bound(Chord::ctrl(Key::Left)), None);
    }

    #[test]
    fn keys_the_page_needs_are_left_alone() {
        for letter in ['A', 'C', 'V', 'X', 'Z'] {
            assert_eq!(bound(Chord::ctrl(Key::Letter(letter))), None, "Ctrl+{letter}");
        }
    }

    #[test]
    fn every_action_is_listed_once_and_every_default_is_accepted() {
        let mut seen = HashSet::new();
        let mut chords = HashSet::new();
        for (shortcut, defaults) in DEFAULTS {
            assert!(seen.insert(*shortcut), "{shortcut:?} listed twice");
            for chord in *defaults {
                assert_eq!(chord.refusal(), None, "{chord}");
                assert!(chords.insert(*chord), "{chord} bound twice");
            }
        }
    }

    #[test]
    fn chords_are_written_and_read_in_one_form() {
        for (text, chord) in [
            ("Ctrl+Shift+T", Chord::ctrl_shift(Key::Letter('T'))),
            ("Alt+Shift+N", Chord::alt_shift(Key::Letter('N'))),
            ("Shift+Esc", Chord::shift(Key::Escape)),
            ("F12", Chord::plain(Key::Function(12))),
            ("Ctrl+9", Chord::ctrl(Key::Digit(9))),
            ("Alt+PageDown", Chord::alt(Key::PageDown)),
        ] {
            assert_eq!(chord.to_string(), text);
            assert_eq!(Chord::parse(text), Some(chord), "{text}");
        }
        assert_eq!(
            Chord::parse(" shift + ctrl + t"),
            Some(Chord::ctrl_shift(Key::Letter('T')))
        );
        for unreadable in [
            "",
            "Ctrl+",
            "Ctrl+Meta+T",
            "Ctrl+F25",
            "Ctrl+F0",
            "Ctrl+,",
            "Ctrl+Space",
        ] {
            assert_eq!(Chord::parse(unreadable), None, "{unreadable:?}");
        }
    }

    #[test]
    fn chords_that_would_take_typing_or_editing_from_pages_are_refused() {
        for (chord, refusal) in [
            (Chord::plain(Key::Letter('T')), ChordRefusal::NeedsModifier),
            (Chord::shift(Key::Letter('T')), ChordRefusal::NeedsModifier),
            (Chord::plain(Key::Escape), ChordRefusal::NeedsModifier),
            (Chord::plain(Key::Tab), ChordRefusal::NeedsModifier),
            (Chord::shift(Key::Tab), ChordRefusal::NeedsModifier),
            (Chord::shift(Key::Delete), ChordRefusal::NeedsModifier),
            (Chord::ctrl(Key::Letter('C')), ChordRefusal::Reserved),
            (Chord::ctrl_shift(Key::Letter('Z')), ChordRefusal::Reserved),
            (Chord::ctrl(Key::Insert), ChordRefusal::Reserved),
            (Chord::ctrl_shift(Key::Left), ChordRefusal::Reserved),
            (Chord::ctrl(Key::End), ChordRefusal::Reserved),
            (Chord::alt(Key::Function(4)), ChordRefusal::Reserved),
        ] {
            assert_eq!(chord.refusal(), Some(refusal), "{chord}");
        }
        for accepted in [
            Chord::alt(Key::Letter('C')),
            Chord::ctrl(Key::Letter('B')),
            Chord::alt(Key::Up),
            Chord::plain(Key::Function(1)),
            Chord::shift(Key::Escape),
            Chord::ctrl(Key::Function(4)),
        ] {
            assert_eq!(accepted.refusal(), None, "{accepted}");
        }
    }

    #[test]
    fn a_changed_action_replaces_its_defaults_and_takes_its_chords_from_others() {
        let changed = BTreeMap::from([
            (Shortcut::Reload, vec![Chord::ctrl(Key::Letter('T'))]),
            (Shortcut::Print, vec![]),
        ]);
        let keymap = Keymap::new(&changed);
        assert_eq!(
            keymap.shortcut_for(Chord::ctrl(Key::Letter('T'))),
            Some(Shortcut::Reload)
        );
        assert_eq!(keymap.shortcut_for(Chord::ctrl(Key::Letter('R'))), None);
        assert_eq!(keymap.shortcut_for(Chord::plain(Key::Function(5))), None);
        assert_eq!(keymap.shortcut_for(Chord::ctrl(Key::Letter('P'))), None);
        let new_tab = keymap.bindings().find(|(shortcut, _)| *shortcut == Shortcut::NewTab);
        assert_eq!(new_tab, Some((Shortcut::NewTab, &[][..])));
    }
}
