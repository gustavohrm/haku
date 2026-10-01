use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::browser::BrowserState;
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
