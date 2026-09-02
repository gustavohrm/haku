pub mod inject;

use std::sync::Arc;

use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::browser::{Effect, BLANK_URL};
use crate::error::{HakuError, Result};
use crate::model::SlotId;
use crate::platform;

/// Label of the webview that renders Haku's own interface.
///
/// It is the window's original webview, so it shares the window label.
pub const CHROME_LABEL: &str = "main";

pub const MAIN_WINDOW_LABEL: &str = "main";

/// Desktop user agent. The system webview would otherwise announce itself with
/// an Edge/WebView2 string that some sites treat as an unsupported browser.
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36"
);

/// Where content webviews sit inside the window, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Viewport {
    #[specta(type = specta_typescript::Number)]
    pub x: f64,
    #[specta(type = specta_typescript::Number)]
    pub y: f64,
    #[specta(type = specta_typescript::Number)]
    pub width: f64,
    #[specta(type = specta_typescript::Number)]
    pub height: f64,
}

/// What a content webview turned out to be showing.
///
/// The two fields arrive separately: the URL is known when the page finishes
/// loading, the title only once the document sets one.
#[derive(Clone, Debug, Default)]
pub struct PageUpdate {
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Receives page updates observed from Rust, with no page involvement.
///
/// Supplied by the caller so this module stays free of application state: it
/// knows how to drive webviews, not what to do with what they report.
pub type PageObserver<R> = Arc<dyn Fn(&tauri::AppHandle<R>, SlotId, PageUpdate) + Send + Sync>;

fn rect_of(viewport: Viewport) -> tauri::Rect {
    tauri::Rect {
        position: tauri::Position::Logical(LogicalPosition::new(viewport.x, viewport.y)),
        size: tauri::Size::Logical(LogicalSize::new(viewport.width, viewport.height)),
    }
}

pub fn chrome<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<tauri::Webview<R>> {
    app.get_webview(CHROME_LABEL)
        .ok_or_else(|| HakuError::WindowMissing(CHROME_LABEL.into()))
}

/// Applies the effects [`crate::browser::Browser`] produced to real webviews.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the main window is gone, and
/// propagates Tauri failures from the individual operations.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    effects: &[Effect],
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    for effect in effects {
        match effect {
            Effect::EnsureSlot { slot, url } => ensure_slot(app, *slot, url, viewport, observer)?,
            Effect::Blank { slot } => navigate(app, *slot, BLANK_URL)?,
            Effect::Destroy { slot } => destroy(app, *slot)?,
            Effect::Show { slot } => set_visible(app, *slot, true)?,
            Effect::Hide { slot } => set_visible(app, *slot, false)?,
            Effect::Reload { slot } => reload(app, *slot)?,
            // The page restores its own scroll offset, so a reload needs no
            // help from here.
            Effect::RestoreScroll { .. } => {}
        }
    }
    Ok(())
}

/// Creates the slot's webview if it does not exist yet, otherwise navigates it.
fn ensure_slot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    url: &str,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    if app.get_webview(&slot.label()).is_some() {
        return navigate(app, slot, url);
    }

    let parsed = parse_url(url)?;
    let window = app
        .get_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| HakuError::WindowMissing(MAIN_WINDOW_LABEL.into()))?;

    // Page metadata is observed from Rust rather than reported by the page, so
    // a remote origin never needs a channel into the application.
    let on_load = {
        let observer = observer.clone();
        let app = app.clone();
        move |_: tauri::Webview<R>, payload: PageLoadPayload<'_>| {
            if payload.event() == PageLoadEvent::Finished {
                observer(&app, slot, PageUpdate { url: Some(payload.url().to_string()), title: None });
            }
        }
    };

    let on_title = {
        let observer = observer.clone();
        let app = app.clone();
        move |_: tauri::Webview<R>, title: String| {
            observer(&app, slot, PageUpdate { url: None, title: Some(title) });
        }
    };

    let builder = tauri::webview::WebviewBuilder::new(slot.label(), WebviewUrl::External(parsed))
        .user_agent(USER_AGENT)
        .devtools(true)
        .initialization_script(inject::scroll_memory_script())
        .on_page_load(on_load)
        .on_document_title_changed(on_title);

    window.add_child(
        builder,
        LogicalPosition::new(viewport.x, viewport.y),
        LogicalSize::new(viewport.width, viewport.height),
    )?;

    // A new child webview is placed above everything, including the chrome, so
    // the chrome has to be lifted back over it. Without this the interface
    // disappears behind the page the moment a slot is created.
    platform::raise_chrome(&chrome(app)?)?;
    Ok(())
}

fn navigate<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, url: &str) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    webview.navigate(parse_url(url)?)?;
    Ok(())
}

fn reload<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId) -> Result<()> {
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.reload()?;
    }
    Ok(())
}

fn destroy<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId) -> Result<()> {
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.close()?;
    }
    Ok(())
}

fn set_visible<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, visible: bool) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    if visible {
        webview.show()?;
    } else {
        webview.hide()?;
    }
    Ok(())
}

/// Moves every content webview to the viewport rectangle the chrome reports.
pub fn set_bounds<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slots: &[SlotId], viewport: Viewport) -> Result<()> {
    for slot in slots {
        if let Some(webview) = app.get_webview(&slot.label()) {
            webview.set_bounds(rect_of(viewport))?;
        }
    }
    Ok(())
}

fn parse_url(url: &str) -> Result<tauri::Url> {
    tauri::Url::parse(url).map_err(|error| HakuError::InvalidUrl(format!("{url}: {error}")))
}
