use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::browser::BrowserState;
use crate::model::{ContextMenuRequestId, ContextMenuTarget, Extensions, NativeMenuItem, TabId};
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

/// Emitted once the extensions folder has been installed, which happens after
/// the interface may already have asked for the list.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct ExtensionsChanged(pub Extensions);

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
