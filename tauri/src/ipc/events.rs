use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::browser::{BrowserState, Ending};
use crate::error::HakuError;
use crate::model::{ContextMenuRequestId, ContextMenuTarget, Download, Extensions, NativeMenuItem, TabId};
use crate::state::MemoryReport;
use crate::storage::Settings;

/// Emitted whenever tabs, activation or pool occupancy change.
///
/// The interface holds no tab state of its own; it renders this. One coarse
/// event rather than many fine ones keeps the frontend from having to reassemble
/// a consistent picture out of a stream of partial updates.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct StateChanged(pub BrowserState);

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct SettingsChanged(pub Settings);

/// Emitted on every tick with what memory looks like now.
///
/// Separate from [`StateChanged`] because the figures move on every tick while
/// the tabs mostly do not, and a tick that changes no tab writes nothing.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct MemoryChanged(pub MemoryReport);

/// Asks the interface to focus the address field, for the given tab.
///
/// Sent by a shortcut. Carries the tab because a shortcut that opens a tab
/// sends it before the interface has rendered that tab.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct AddressFocusRequested(pub TabId);

/// Asks the interface to open the find bar, for the given tab.
/// [Find, print and save § Contract](../../../docs/specs/find-print-save.md#contract).
///
/// Sent by Ctrl+F, to the tab's window only.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct FindRequested(pub TabId);

/// A tab's page started a navigation, reload included, which ends its find.
/// [Find, print and save § Contract](../../../docs/specs/find-print-save.md#contract).
///
/// Sent to the tab's window only, for every top-frame navigation: Rust keeps
/// no find state to know whether one was open.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct FindEnded(pub TabId);

/// A page's find moved or recounted.
/// [Find, print and save § Contract](../../../docs/specs/find-print-save.md#contract).
///
/// Sent to the tab's window only.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub tab: TabId,
    /// 0 when nothing matches.
    pub matches: u32,
    /// The current match, counting from 1.
    pub active: Option<u32>,
}

/// A failure no command call is waiting on, such as a shortcut's, a page's
/// request for a window, or one at startup.
/// [Errors § Failures no command is waiting on](../../../docs/specs/errors.md#failures-no-command-is-waiting-on).
///
/// Sent to one window only, so it is shown once.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct FailureReported(pub HakuError);

/// Emitted once the extensions folder has been installed, which happens after
/// the interface may already have asked for the list.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct ExtensionsChanged(pub Extensions);

/// The downloads a window lists, newest first, whenever one starts, moves
/// on or is unlisted. [Downloads § Contract](../../../docs/specs/downloads.md#contract).
///
/// Each window receives only its own profile's downloads. A running download's
/// progress is sent at most four times a second.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct DownloadsChanged(pub Vec<Download>);

/// Closing a tab or window would cancel downloads, so it was not done: the
/// interface asks, and answers with
/// [`confirm_close`](super::commands::confirm_close) to close anyway.
/// [Downloads § Closing](../../../docs/specs/downloads.md#closing).
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CloseConfirmationRequested {
    /// The tab being closed, or nothing when it is the window.
    pub tab: Option<TabId>,
    pub ending: Ending,
    /// How many downloads closing would cancel.
    #[specta(type = specta_typescript::Number)]
    pub downloads: usize,
}

/// A page was right-clicked, and waits for the menu the interface draws.
/// [Context menus § Contract](../../../docs/specs/context-menus.md#contract).
///
/// Sent to the tab's window only. The interface answers with
/// [`answer_context_menu`](super::commands::answer_context_menu).
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuRequested {
    pub request: ContextMenuRequestId,
    pub tab: TabId,
    /// Where the pointer was, in the chrome's CSS pixels.
    #[specta(type = specta_typescript::Number)]
    pub x: f64,
    #[specta(type = specta_typescript::Number)]
    pub y: f64,
    pub target: ContextMenuTarget,
    /// The allowed native items only, in the engine's order.
    pub items: Vec<NativeMenuItem>,
}
