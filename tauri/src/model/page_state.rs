//! What a page holds that discarding it would lose.
//!
//! Pages report nothing on their own. An injected script keeps a record inside
//! the page and Rust reads it; see `webview::inject::page_state_script`. A page
//! can lie in its own record, and the most it gains is keeping its own tab in
//! memory, or losing its own draft.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::tab::Scroll;

/// Interactions without a URL change that mean the page holds state a reload
/// would not bring back.
pub const INTERACTION_THRESHOLD: u32 = 10;

/// Largest draft kept per tab, in bytes. A larger one is dropped, not
/// truncated: half a draft restored into a form is worse than none.
pub const DRAFT_LIMIT: usize = 64 * 1024;

/// One reading of a page's record.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageState {
    /// The URL the page was on when read, so a reading that arrives after the
    /// tab moved on is not applied to the wrong entry.
    pub url: String,
    /// The user typed into a field or editable region that still holds it.
    pub unsaved: bool,
    /// The page asks before it is left, and the user has interacted with it.
    pub unload_armed: bool,
    /// Trusted clicks and key presses since the URL last changed.
    pub interactions: u32,
    /// Some long media element is paused partway through.
    pub media_paused: bool,
    /// The page holds a live camera, microphone or screen capture.
    pub capturing: bool,
    pub scroll: Scroll,
    /// The changed form fields, as the page script serialises them.
    pub draft: Option<String>,
}

/// What discarding a tab would cost, from least to most.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Loss {
    /// A reload shows the same thing.
    #[default]
    None,
    /// The page would come back different.
    State,
    /// Something the user made would be gone.
    Work,
}

/// A reason behind a tab's [`Loss`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LossSignal {
    /// The tab's host is one the user asked Haku not to unload.
    KeptSite,
    Unsaved,
    UnloadArmed,
    FormResult,
    Interactions,
    MediaPaused,
    Unreadable,
}

impl LossSignal {
    pub fn level(self) -> Loss {
        match self {
            Self::KeptSite | Self::Unsaved | Self::UnloadArmed | Self::FormResult => Loss::Work,
            Self::Interactions | Self::MediaPaused | Self::Unreadable => Loss::State,
        }
    }
}

/// Everything known about a tab's page beyond its URL.
///
/// Belongs to the page in the tab's current history entry: it is cleared when
/// the tab moves to another one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageRecord {
    /// The last reading, or nothing when the page has not been read.
    pub state: Option<PageState>,
    /// The last attempt to read the page failed or timed out.
    pub unreadable: bool,
    /// The page is the result of a form submission.
    pub form_result: bool,
}

impl PageRecord {
    pub fn capturing(&self) -> bool {
        self.state.as_ref().is_some_and(|state| state.capturing)
    }

    /// The reasons discarding the page would cost something.
    pub fn signals(&self) -> Vec<LossSignal> {
        let mut signals = Vec::new();
        if let Some(state) = &self.state {
            if state.unsaved {
                signals.push(LossSignal::Unsaved);
            }
            if state.unload_armed {
                signals.push(LossSignal::UnloadArmed);
            }
        }
        if self.form_result {
            signals.push(LossSignal::FormResult);
        }
        if let Some(state) = &self.state {
            if state.interactions >= INTERACTION_THRESHOLD {
                signals.push(LossSignal::Interactions);
            }
            if state.media_paused {
                signals.push(LossSignal::MediaPaused);
            }
        }
        if self.unreadable {
            signals.push(LossSignal::Unreadable);
        }
        signals
    }

    /// The highest level any signal reaches.
    pub fn loss(&self) -> Loss {
        self.signals()
            .into_iter()
            .map(LossSignal::level)
            .max()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(state: PageState) -> PageRecord {
        PageRecord {
            state: Some(state),
            ..PageRecord::default()
        }
    }

    #[test]
    fn a_page_never_read_loses_nothing() {
        assert_eq!(PageRecord::default().loss(), Loss::None);
    }

    #[test]
    fn unsaved_text_is_work() {
        let record = read(PageState {
            unsaved: true,
            ..PageState::default()
        });
        assert_eq!(record.loss(), Loss::Work);
    }

    #[test]
    fn an_armed_unload_prompt_is_work() {
        let record = read(PageState {
            unload_armed: true,
            ..PageState::default()
        });
        assert_eq!(record.loss(), Loss::Work);
    }

    #[test]
    fn a_form_result_is_work() {
        let record = PageRecord {
            form_result: true,
            ..PageRecord::default()
        };
        assert_eq!(record.loss(), Loss::Work);
    }

    #[test]
    fn interactions_become_state_at_the_threshold() {
        let below = read(PageState {
            interactions: INTERACTION_THRESHOLD - 1,
            ..PageState::default()
        });
        let at = read(PageState {
            interactions: INTERACTION_THRESHOLD,
            ..PageState::default()
        });
        assert_eq!(below.loss(), Loss::None);
        assert_eq!(at.loss(), Loss::State);
    }

    #[test]
    fn paused_media_is_state() {
        let record = read(PageState {
            media_paused: true,
            ..PageState::default()
        });
        assert_eq!(record.loss(), Loss::State);
    }

    #[test]
    fn a_page_that_could_not_be_read_is_state() {
        let record = PageRecord {
            unreadable: true,
            ..PageRecord::default()
        };
        assert_eq!(record.loss(), Loss::State);
    }

    #[test]
    fn the_highest_level_wins_and_every_signal_is_named() {
        let record = PageRecord {
            unreadable: true,
            ..read(PageState {
                unsaved: true,
                media_paused: true,
                ..PageState::default()
            })
        };
        assert_eq!(record.loss(), Loss::Work);
        assert_eq!(
            record.signals(),
            vec![LossSignal::Unsaved, LossSignal::MediaPaused, LossSignal::Unreadable]
        );
    }

    #[test]
    fn capture_is_not_a_loss() {
        let record = read(PageState {
            capturing: true,
            ..PageState::default()
        });
        assert!(record.capturing());
        assert_eq!(record.loss(), Loss::None);
    }
}
