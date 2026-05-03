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
})();
"#;

#[derive(Clone, serde::Serialize)]
struct TabInfo {
    title: String,
    favicon: String,
    url: String,
}

#[tauri::command]
fn update_tab_info(app: tauri::AppHandle, title: String, favicon: String, url: String) -> Result<(), String> {
    app.emit("tab-info", TabInfo { title, favicon, url })
        .map_err(|e| e.to_string())
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
fn browser_go_back(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .eval("history.back()")
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn browser_go_forward(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
        .ok_or_else(|| "Browser webview not found.".to_string())?
        .eval("history.forward()")
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
            browser_go_back,
            browser_go_forward,
            open_browser_devtools,
            update_tab_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
