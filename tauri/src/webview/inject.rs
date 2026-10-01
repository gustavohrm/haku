//! Scripts that run inside content webviews.
//!
//! ## Why nothing here talks to Rust
//!
//! Content webviews load arbitrary remote pages. Tauri v2 withholds its IPC
//! bridge from remote origins unless a capability explicitly opts them in, and
//! that default is right: a channel from any page to the application's commands
//! is a large attack surface to open for the sake of convenience.
//!
//! So the flow only ever goes the other way: a script here keeps a record inside
//! the page, and Rust reads it by evaluating an expression in the page when the
//! webview reports that something changed. The page can see and tamper with that
//! record, but all it could corrupt is its own tab's history, and it gains no
//! way to call into Haku.

use crate::model::{Commit, NavigationKind};

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

/// Name of the page-side function that hands over the navigation log.
const NAVIGATION_READER: &str = "__hakuNavigation";

/// Records every URL change the page makes, with the kind of change.
///
/// Whether a new URL should add a history entry depends on how the page got
/// there, and only the page knows: a single-page app's `pushState` and
/// `replaceState` never load a document, and a redirect looks like any other
/// load from outside. The Navigation API reports exactly that, as `push`,
/// `replace`, `traverse` or `reload`, both for the load that created the
/// document and for every same-document change after it.
///
/// Runs in the top frame only; frames do not own the tab's URL.
pub fn navigation_log_script() -> String {
    format!(
        r#"
(function () {{
  if (window.top !== window || !window.navigation || window.{NAVIGATION_READER}) return;
  var log = [];
  function record(type) {{
    log.push({{ type: type || "replace", url: location.href }});
  }}
  record(navigation.activation ? navigation.activation.navigationType : "push");
  navigation.addEventListener("currententrychange", function (event) {{
    record(event.navigationType);
  }});
  Object.defineProperty(window, "{NAVIGATION_READER}", {{
    value: function () {{
      return {{ log: log.splice(0), title: document.title }};
    }},
  }});
}})();
"#
    )
}

/// Expression Rust evaluates to collect the log recorded by
/// [`navigation_log_script`], emptying it.
///
/// Evaluates to `null` where the script never ran, such as an error page or
/// the built-in PDF viewer; the caller then falls back to the webview's URL.
pub fn navigation_drain_expression() -> String {
    format!(r#"typeof window.{NAVIGATION_READER} === "function" ? window.{NAVIGATION_READER}() : null"#)
}

#[derive(serde::Deserialize)]
struct DrainedLog {
    log: Vec<DrainedEntry>,
    title: String,
}

#[derive(serde::Deserialize)]
struct DrainedEntry {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

/// Reads what [`navigation_drain_expression`] evaluated to.
///
/// @param json - The evaluation result, as JSON.
/// @returns The commits in order and the document title, or `None` when the
/// page had no log to hand over.
pub fn parse_navigation_log(json: &str) -> Option<(Vec<Commit>, String)> {
    let drained: DrainedLog = serde_json::from_str(json).ok()?;
    let commits = drained
        .log
        .into_iter()
        .map(|entry| Commit { url: entry.url, kind: NavigationKind::parse(&entry.kind) })
        .collect();
    Some((commits, drained.title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drained_log_keeps_the_order_and_kind_of_every_change() {
        let json = r#"{"log":[{"type":"push","url":"https://a.test/1"},{"type":"replace","url":"https://a.test/2"}],"title":"A"}"#;

        let (commits, title) = parse_navigation_log(json).unwrap();

        assert_eq!(
            commits,
            vec![
                Commit { url: "https://a.test/1".into(), kind: NavigationKind::Push },
                Commit { url: "https://a.test/2".into(), kind: NavigationKind::Replace },
            ]
        );
        assert_eq!(title, "A");
    }

    #[test]
    fn a_page_without_the_log_yields_nothing_so_the_caller_can_fall_back() {
        assert!(parse_navigation_log("null").is_none());
        assert!(parse_navigation_log("not json").is_none());
    }
}
