use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};

const BROWSER_WEBVIEW_LABEL: &str = "browser-webview";

#[derive(Clone, Copy, Debug, serde::Deserialize)]
struct BrowserBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

const PAGE_INFO_SCRIPT: &str = r#"
(function () {
    function notify() {
        if (!/^https?:\/\//i.test(location.href)) return;
        var favicon = '';
        var link = document.querySelector('link[rel~="icon"]') || document.querySelector('link[rel="shortcut icon"]');
        if (link) favicon = link.href;
        if (window.__TAURI__ && window.__TAURI__.core) {
            window.__TAURI__.core.invoke('update_tab_info', {
                title: document.title,
                favicon: favicon,
                url: location.href
            }).catch(function () {});
        }
    }
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', notify);
    } else {
        notify();
    }
    window.addEventListener('load', notify);
    window.addEventListener('popstate', notify);
    window.addEventListener('hashchange', notify);
    var _push = history.pushState.bind(history);
    var _replace = history.replaceState.bind(history);
    history.pushState = function() { _push.apply(this, arguments); notify(); };
    history.replaceState = function() { _replace.apply(this, arguments); notify(); };
    var _scrollTimer = null;
    window.addEventListener('scroll', function() {
        if (_scrollTimer) clearTimeout(_scrollTimer);
        _scrollTimer = setTimeout(function() {
            if (window.__TAURI__ && window.__TAURI__.core) {
                window.__TAURI__.core.invoke('update_scroll_position', {
                    x: window.scrollX,
                    y: window.scrollY
                }).catch(function() {});
            }
        }, 150);
    }, { passive: true });
})();
"#;

#[derive(Clone, serde::Serialize)]
struct TabInfo {
    title: String,
    favicon: String,
    url: String,
}

#[derive(Clone, serde::Serialize)]
struct ScrollPosition {
    x: f64,
    y: f64,
}

#[tauri::command]
fn update_tab_info(app: tauri::AppHandle, title: String, favicon: String, url: String) -> Result<(), String> {
    app.emit("tab-info", TabInfo { title, favicon, url })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_scroll_position(app: tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    app.emit("scroll-position", ScrollPosition { x, y })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn scroll_browser_to(app: tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .eval(&format!("window.scrollTo({x}, {y})"))
        .map_err(|error| error.to_string())
}

fn parse_url(url: &str) -> Result<tauri::Url, String> {
    tauri::Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))
}

fn browser_rect(bounds: BrowserBounds) -> tauri::Rect {
    tauri::Rect {
        position: tauri::Position::Logical(LogicalPosition::new(bounds.x, bounds.y)),
        size: tauri::Size::Logical(LogicalSize::new(bounds.width, bounds.height)),
    }
}

#[tauri::command]
async fn ensure_browser_webview(
    app: tauri::AppHandle,
    url: String,
    bounds: BrowserBounds,
) -> Result<(), String> {
    let parsed_url = parse_url(&url)?;
    let rect = browser_rect(bounds);

    if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        webview
            .set_bounds(rect)
            .map_err(|error| error.to_string())?;
        webview
            .navigate(parsed_url)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| "Main window not found.".to_string())?;
    let builder = tauri::webview::WebviewBuilder::new(
        BROWSER_WEBVIEW_LABEL,
        WebviewUrl::External(parsed_url),
    )
    .devtools(true)
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36")
    .initialization_script(PAGE_INFO_SCRIPT);

    window
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn set_browser_bounds(app: tauri::AppHandle, bounds: BrowserBounds) -> Result<(), String> {
    if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        webview
            .set_bounds(browser_rect(bounds))
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn navigate_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .navigate(parse_url(&url)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reload_browser(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .reload()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_browser_devtools(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .open_devtools();

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ensure_browser_webview,
            set_browser_bounds,
            navigate_browser,
            reload_browser,
            open_browser_devtools,
            update_tab_info,
            update_scroll_position,
            scroll_browser_to
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
