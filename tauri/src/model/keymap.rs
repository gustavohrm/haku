//! Browser shortcuts: which keys do what.
//!
//! There is one keymap, and it is read natively, whichever webview has focus,
//! so a shortcut behaves the same over a page as over the interface. Keys it
//! does not name reach the page or the interface untouched.

/// A key a shortcut can be bound to, independent of the platform's codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    PageUp,
    PageDown,
}

/// A key pressed with the modifiers held at the time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
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
}

/// What a shortcut asks the browser to do, always to the active tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    NewTab,
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    /// The tab at this position, counting from zero.
    NthTab(usize),
    LastTab,
    FocusAddress,
    Reload,
    Back,
    Forward,
    DevTools,
}

/// The shortcut a chord is bound to, if any.
///
/// Modifiers must match exactly, so Ctrl+Shift+W is not Ctrl+W. The bindings
/// follow Chrome's, since that is what the hands already know.
pub fn shortcut_for(chord: Chord) -> Option<Shortcut> {
    use Key::{Digit, Function, Left, Letter, PageDown, PageUp, Right, Tab};

    let Chord { key, ctrl, shift, alt } = chord;
    let shortcut = match (ctrl, shift, alt, key) {
        (true, false, false, Letter('T')) => Shortcut::NewTab,
        (true, false, false, Letter('W') | Function(4)) => Shortcut::CloseTab,
        (true, true, false, Letter('T')) => Shortcut::ReopenClosedTab,
        (true, false, false, Tab | PageDown) => Shortcut::NextTab,
        (true, true, false, Tab) | (true, false, false, PageUp) => Shortcut::PreviousTab,
        (true, false, false, Digit(9)) => Shortcut::LastTab,
        (true, false, false, Digit(n @ 1..=8)) => Shortcut::NthTab(usize::from(n - 1)),
        (true, false, false, Letter('L')) | (false, false, true, Letter('D')) | (false, false, false, Function(6)) => {
            Shortcut::FocusAddress
        }
        (true, false, false, Letter('R')) | (false, false, false, Function(5)) => Shortcut::Reload,
        (false, false, true, Left) => Shortcut::Back,
        (false, false, true, Right) => Shortcut::Forward,
        (false, false, false, Function(12)) | (true, true, false, Letter('I')) => Shortcut::DevTools,
        _ => return None,
    };
    Some(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_shortcuts_follow_chrome() {
        assert_eq!(shortcut_for(Chord::ctrl(Key::Letter('T'))), Some(Shortcut::NewTab));
        assert_eq!(shortcut_for(Chord::ctrl(Key::Letter('W'))), Some(Shortcut::CloseTab));
        assert_eq!(
            shortcut_for(Chord::ctrl_shift(Key::Letter('T'))),
            Some(Shortcut::ReopenClosedTab)
        );
        assert_eq!(shortcut_for(Chord::ctrl(Key::Tab)), Some(Shortcut::NextTab));
        assert_eq!(shortcut_for(Chord::ctrl_shift(Key::Tab)), Some(Shortcut::PreviousTab));
    }

    #[test]
    fn digits_select_by_position_and_nine_selects_the_last() {
        assert_eq!(shortcut_for(Chord::ctrl(Key::Digit(1))), Some(Shortcut::NthTab(0)));
        assert_eq!(shortcut_for(Chord::ctrl(Key::Digit(8))), Some(Shortcut::NthTab(7)));
        assert_eq!(shortcut_for(Chord::ctrl(Key::Digit(9))), Some(Shortcut::LastTab));
        assert_eq!(shortcut_for(Chord::ctrl(Key::Digit(0))), None);
    }

    #[test]
    fn alt_and_function_key_bindings_follow_chrome() {
        assert_eq!(shortcut_for(Chord::alt(Key::Letter('D'))), Some(Shortcut::FocusAddress));
        assert_eq!(shortcut_for(Chord::alt(Key::Left)), Some(Shortcut::Back));
        assert_eq!(shortcut_for(Chord::alt(Key::Right)), Some(Shortcut::Forward));
        assert_eq!(shortcut_for(Chord::ctrl(Key::Function(4))), Some(Shortcut::CloseTab));
        assert_eq!(shortcut_for(Chord::plain(Key::Function(5))), Some(Shortcut::Reload));
        assert_eq!(
            shortcut_for(Chord::plain(Key::Function(6))),
            Some(Shortcut::FocusAddress)
        );
        assert_eq!(shortcut_for(Chord::plain(Key::Function(12))), Some(Shortcut::DevTools));
        assert_eq!(
            shortcut_for(Chord::ctrl_shift(Key::Letter('I'))),
            Some(Shortcut::DevTools)
        );
        assert_eq!(shortcut_for(Chord::ctrl(Key::PageDown)), Some(Shortcut::NextTab));
        assert_eq!(shortcut_for(Chord::ctrl(Key::PageUp)), Some(Shortcut::PreviousTab));
    }

    #[test]
    fn modifiers_must_match_exactly() {
        assert_eq!(shortcut_for(Chord::ctrl_shift(Key::Letter('W'))), None);
        assert_eq!(shortcut_for(Chord::plain(Key::Letter('T'))), None);
        assert_eq!(shortcut_for(Chord::ctrl(Key::Left)), None);
    }

    #[test]
    fn keys_the_page_needs_are_left_alone() {
        for letter in ['A', 'C', 'V', 'X', 'Z', 'F', 'P'] {
            assert_eq!(shortcut_for(Chord::ctrl(Key::Letter(letter))), None, "Ctrl+{letter}");
        }
    }
}
