use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::browser::Browser;
use crate::chrome::Layout;
use crate::error::{HakuError, Result};
use crate::storage::{HistoryDb, Paths, Session, Settings};
use crate::webview::Viewport;

/// Milliseconds since the Unix epoch.
///
/// Used for history timestamps and for when a tab was last shown.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// Everything the command layer needs, behind the locks that make it shareable.
///
/// The browser is the only mutable authority over tabs. Commands take the write
/// lock, mutate, collect effects, and release it before touching webviews, so a
/// slow native call never blocks the next command.
pub struct AppState {
    pub browser: RwLock<Browser>,
    pub settings: RwLock<Settings>,
    pub history: Mutex<HistoryDb>,
    pub layout: RwLock<Layout>,
    /// The last rectangle page content was given.
    ///
    /// An internal page reports no viewport, because the chrome covers the whole
    /// window while one is open. Content webviews still belong at the last real
    /// rectangle, so remembering it keeps a webview created while an internal
    /// page is showing from being built at zero size and staying invisible.
    last_viewport: RwLock<Viewport>,
    pub paths: Paths,
}

impl AppState {
    pub fn new(browser: Browser, settings: Settings, history: HistoryDb, paths: Paths) -> Self {
        Self {
            browser: RwLock::new(browser),
            settings: RwLock::new(settings),
            history: Mutex::new(history),
            layout: RwLock::new(Layout::default()),
            last_viewport: RwLock::new(Viewport::default()),
            paths,
        }
    }

    /// Where content webviews belong.
    ///
    /// Zero until the interface has reported a layout, which keeps a webview
    /// from being created at a meaningless position during startup.
    pub fn viewport(&self) -> Viewport {
        self.last_viewport.read().map(|viewport| *viewport).unwrap_or_default()
    }

    /// Records a reported layout, keeping the last real viewport.
    pub fn set_layout(&self, layout: Layout) {
        if let Some(viewport) = layout.viewport {
            if let Ok(mut last) = self.last_viewport.write() {
                *last = viewport;
            }
        }
        if let Ok(mut current) = self.layout.write() {
            *current = layout;
        }
    }

    /// Persists the open tabs so the next launch restores them.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when the session file cannot be written.
    pub fn save_session(&self) -> Result<()> {
        let browser = self
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        crate::storage::write_json(&self.paths.session, &Session::capture(&browser))
    }

    /// # Errors
    /// Returns [`HakuError::Storage`] when the settings file cannot be written.
    pub fn save_settings(&self) -> Result<()> {
        let settings = self
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        crate::storage::write_json(&self.paths.settings, &*settings)
    }
}
