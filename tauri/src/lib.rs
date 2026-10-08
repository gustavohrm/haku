//! Haku: a lightweight desktop browser.
//!
//! ## Shape
//!
//! Rust owns the browser. [`browser::Browser`] holds every tab, its history and
//! the webview pool, and it is the only thing allowed to change them. It is
//! also entirely free of Tauri: it decides what should be true and returns
//! [`browser::Effect`]s describing the difference, which the [`webview`] module
//! applies to real webviews. That split is what makes tab and pool behaviour
//! testable without a window.
//!
//! The interface is a React application in each window's chrome webview. It
//! holds no tab state of its own; it sends intents and renders the state
//! events it receives, so every window renders the one browser.
//!
//! ## Layering
//!
//! The chrome webview covers the whole window and is held above every content
//! webview, so the viewport can have rounded corners and menus can overlap the
//! page. The interface reports what it occupies and [`chrome`] turns that into
//! a native input mask, letting clicks through to the page everywhere the
//! interface is not. See [`platform`] for why this needs native code.

pub mod browser;
pub mod chrome;
pub mod error;
pub mod ipc;
pub mod model;
pub mod platform;
pub mod state;
pub mod storage;
pub mod webview;

use tauri::Manager;

use crate::model::{Preset, WindowKind};
use crate::state::AppState;
use crate::storage::{HistoryDb, Paths, Session, Settings};
use crate::webview::Frame;

/// Builds and runs the application.
///
/// # Panics
/// Panics if the application data directory cannot be resolved or the window
/// cannot be created; neither is recoverable, and continuing would leave a
/// browser that cannot store anything or show anything.
pub fn run() {
    let ipc = ipc::builder();

    tauri::Builder::default()
        .invoke_handler(ipc.invoke_handler())
        .on_window_event(ipc::commands::on_window_event)
        .setup(move |app| {
            ipc.mount_events(app);

            let root = app.path().app_data_dir().expect("no application data directory");
            let paths = Paths::under(&root);

            // A first launch starts on the Balanced preset rather than on the
            // bare defaults, which keep a single page loaded.
            let first_launch = !paths.settings.exists();
            let mut settings: Settings = storage::read_json::<Settings>(&paths.settings).sanitized();
            if first_launch {
                settings = settings.with_preset(Preset::Balanced);
            }
            let session: Session = storage::read_json(&paths.session);
            let history = HistoryDb::open(&paths.history).expect("could not open the history database");

            let (browser, restored) = session.restore(settings.pool_capacity(), &settings.home_url);
            let browser = browser
                .with_policies(settings.freeze_tabs, settings.discard_tabs)
                .with_keeping(settings.kept_memory_bytes(), settings.kept_sites.clone());
            let windows: Vec<_> = browser.window_ids().into_iter().zip(restored).collect();
            app.manage(AppState::new(browser, settings, history, paths));
            if first_launch {
                let _ = app.state::<AppState>().save_settings();
            }

            // A window that is not moved before the next save keeps its place.
            if let Ok(mut bounds) = app.state::<AppState>().bounds.write() {
                for (window, restored) in &windows {
                    if let Some(restored) = restored.bounds {
                        bounds.insert(*window, restored);
                    }
                }
            }
            for (window, restored) in &windows {
                let frame = restored.bounds.map_or(Frame::Maximized, Frame::Bounds);
                webview::build_window(app.handle(), *window, WindowKind::Normal, frame)?;
            }
            ipc::commands::tick_periodically(app.handle().clone());

            // Each chrome starts underneath any content webview created later,
            // so it is lifted once here and again after every slot is created,
            // and takes the browser's shortcuts while the interface has focus.
            // The first also watches for service workers left running by pages
            // that have gone, which would otherwise hold their memory
            // indefinitely, and installs the extensions folder.
            // Setup runs on the main thread and raising dispatches back to it,
            // so this has to happen off that thread or it would wait forever.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let keys = ipc::commands::hooks().keys;
                for (window, _) in &windows {
                    let _ = webview::prepare_chrome(&handle, *window, &keys);
                }
                if let Ok(chrome) = webview::chrome(&handle) {
                    let _ = platform::stop_idle_workers(&chrome);
                    ipc::commands::sync_extensions(&handle);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Haku");
}
