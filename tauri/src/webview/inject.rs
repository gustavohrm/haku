//! Scripts that run inside content webviews.
//!
//! ## Why nothing here talks to Rust
//!
//! Content webviews load arbitrary remote pages. Tauri v2 withholds its IPC
//! bridge from remote origins unless a capability explicitly opts them in, and
//! that default is right: a channel from any page to the application's commands
//! is a large attack surface to open for the sake of convenience.
//!
//! So page metadata is collected on the Rust side instead, through the
//! `on_page_load` and `on_document_title_changed` hooks, which need no page
//! privileges at all. The only thing that genuinely cannot be done from Rust is
//! reading the scroll offset, and that is handled here without leaving the page.

/// Key prefix for the per-page scroll offset.
const STORAGE_PREFIX: &str = "__haku_scroll__";

/// Milliseconds to coalesce scroll events over.
const SCROLL_DEBOUNCE_MS: u32 = 150;

/// Remembers and restores the scroll position, entirely within the page.
///
/// A suspended tab is reloaded when it comes back, which would otherwise return
/// to the top of the page. The offset is kept in `sessionStorage` so it never
/// leaves the origin it belongs to and is discarded with the browsing session,
/// unlike `localStorage`, which would leave Haku's data on the site permanently.
///
/// The trade-off is that the offset lives in the webview that saved it, so a tab
/// restored into a different pool slot starts at the top.
pub fn scroll_memory_script() -> String {
    format!(
        r#"
(function () {{
  var key = function () {{ return "{STORAGE_PREFIX}" + location.href; }};

  function save() {{
    try {{
      sessionStorage.setItem(key(), window.scrollX + "," + window.scrollY);
    }} catch (_) {{
      // Storage can be unavailable or full; losing a scroll offset is not
      // worth breaking the page over.
    }}
  }}

  function restore() {{
    try {{
      var stored = sessionStorage.getItem(key());
      if (!stored) return;
      var parts = stored.split(",");
      window.scrollTo(parseFloat(parts[0]) || 0, parseFloat(parts[1]) || 0);
    }} catch (_) {{}}
  }}

  var timer = null;
  window.addEventListener(
    "scroll",
    function () {{
      if (timer) clearTimeout(timer);
      timer = setTimeout(save, {SCROLL_DEBOUNCE_MS});
    }},
    {{ passive: true }}
  );

  // Restoring on load alone lands too early for pages that lay out after their
  // own scripts run, so it is attempted again once everything has settled.
  window.addEventListener("load", function () {{
    restore();
    setTimeout(restore, 120);
  }});
  window.addEventListener("pagehide", save);
}})();
"#
    )
}
