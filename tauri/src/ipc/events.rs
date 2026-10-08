use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::browser::BrowserState;
use crate::model::Extensions;
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

/// Emitted once the extensions folder has been installed, which happens after
/// the interface may already have asked for the list.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct ExtensionsChanged(pub Extensions);
