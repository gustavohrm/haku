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
/// A discarded tab is reloaded when it comes back, which would otherwise return
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

/// Key prefix for the per-page form contents.
const FORM_PREFIX: &str = "__haku_form__";

/// Remembers what the user typed into a page's forms, and puts it back when a
/// discarded tab reloads.
///
/// Kept in `sessionStorage`, as scroll is and for the same reasons, with the same
/// limit: a tab restored into a different pool slot starts empty.
///
/// Only fields the user changed are kept, so a reload never overwrites what the
/// page itself filled in, and a field the page has already changed again is left
/// alone. Fields that hold secrets are never stored: passwords, payment details,
/// one-time codes, hidden and file fields, and anything the page marked
/// `autocomplete="off"`. A submitted form's contents are forgotten, since what
/// was submitted is no longer a draft.
///
/// Values are set through the element's own setter and announced with `input`
/// and `change` events, which is what frameworks that own their fields' state
/// listen for.
pub fn form_memory_script() -> String {
    FORM_MEMORY_JS.replace("__PREFIX__", FORM_PREFIX)
}

const FORM_MEMORY_JS: &str = r#"
(function () {
  if (window.top !== window) return;
  var key = function () { return "__PREFIX__" + location.href; };
  var SKIPPED_TYPES = /^(password|hidden|file|submit|button|reset|image)$/i;
  var SECRET_AUTOCOMPLETE = /(^|\s)(off|cc-[a-z-]+|one-time-code|current-password|new-password)(\s|$)/i;
  var SECRET_NAME = /(card.?num|cc.?num|cvv|cvc|csc|iban|security.?code)/i;

  function fields() {
    return Array.prototype.filter.call(
      document.querySelectorAll("input, textarea, select"),
      function (field) {
        if (field.tagName === "INPUT" && SKIPPED_TYPES.test(field.type)) return false;
        var auto = (field.getAttribute("autocomplete") || "") + " " +
          ((field.form && field.form.getAttribute("autocomplete")) || "");
        if (SECRET_AUTOCOMPLETE.test(auto)) return false;
        return !SECRET_NAME.test((field.name || "") + " " + (field.id || ""));
      }
    );
  }

  function checkable(field) {
    return field.type === "checkbox" || field.type === "radio";
  }

  function identity(field) {
    return field.tagName + ":" + (field.type || "") + ":" + (field.name || field.id || "");
  }

  function read(field) {
    if (checkable(field)) return field.checked === field.defaultChecked ? null : { c: field.checked };
    if (field.tagName === "SELECT") {
      var chosen = Array.prototype.map.call(field.options, function (option) { return option.selected; });
      var initial = Array.prototype.map.call(field.options, function (option) { return option.defaultSelected; });
      // A single choice with nothing marked selected shows its first option.
      if (!field.multiple && initial.indexOf(true) === -1 && initial.length) initial[0] = true;
      return chosen.join() === initial.join() ? null : { s: chosen };
    }
    return field.value === field.defaultValue ? null : { v: field.value };
  }

  // Set by a submit and cleared by the next edit, so leaving the page right
  // after submitting does not save the submitted values straight back.
  var submitted = false;

  function save() {
    if (submitted) return;
    try {
      var stored = [];
      fields().forEach(function (field, index) {
        var value = read(field);
        if (value) {
          value.i = index;
          value.k = identity(field);
          stored.push(value);
        }
      });
      if (stored.length) sessionStorage.setItem(key(), JSON.stringify(stored));
      else sessionStorage.removeItem(key());
    } catch (_) {
      // Storage can be unavailable or full; losing a draft is not worth
      // breaking the page over.
    }
  }

  function set(field, property, value) {
    var prototype = Object.getPrototypeOf(field);
    var descriptor = Object.getOwnPropertyDescriptor(prototype, property);
    if (descriptor && descriptor.set) descriptor.set.call(field, value);
    else field[property] = value;
  }

  var restored = false;
  function restore() {
    if (restored) return;
    try {
      var stored = JSON.parse(sessionStorage.getItem(key()) || "null");
      if (!stored) return;
      var current = fields();
      stored.forEach(function (entry) {
        var field = current[entry.i];
        // The page has changed shape, or already holds something of its own.
        if (!field || identity(field) !== entry.k || read(field)) return;
        if ("c" in entry) set(field, "checked", entry.c);
        else if ("s" in entry) {
          Array.prototype.forEach.call(field.options, function (option, index) {
            option.selected = !!entry.s[index];
          });
        } else set(field, "value", entry.v);
        field.dispatchEvent(new Event("input", { bubbles: true }));
        field.dispatchEvent(new Event("change", { bubbles: true }));
        restored = true;
      });
    } catch (_) {}
  }

  var timer = null;
  function schedule(event) {
    // Restoring announces itself with the same events; that is not an edit.
    if (!event.isTrusted) return;
    submitted = false;
    if (timer) clearTimeout(timer);
    timer = setTimeout(save, 300);
  }
  document.addEventListener("input", schedule, true);
  document.addEventListener("change", schedule, true);
  document.addEventListener("submit", function () {
    submitted = true;
    if (timer) clearTimeout(timer);
    try { sessionStorage.removeItem(key()); } catch (_) {}
  }, true);
  window.addEventListener("pagehide", save);

  // Pages that build their forms with script are not done at "load", so the
  // restore is tried again once they have had a moment.
  window.addEventListener("load", function () {
    restore();
    setTimeout(restore, 500);
  });
})();
"#;

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
        .map(|entry| Commit {
            url: entry.url,
            kind: NavigationKind::parse(&entry.kind),
        })
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
                Commit {
                    url: "https://a.test/1".into(),
                    kind: NavigationKind::Push
                },
                Commit {
                    url: "https://a.test/2".into(),
                    kind: NavigationKind::Replace
                },
            ]
        );
        assert_eq!(title, "A");
    }

    #[test]
    fn form_contents_are_kept_under_their_own_prefix() {
        let script = form_memory_script();

        assert!(script.contains(FORM_PREFIX));
        assert!(!script.contains("__PREFIX__"));
    }

    #[test]
    fn a_page_without_the_log_yields_nothing_so_the_caller_can_fall_back() {
        assert!(parse_navigation_log("null").is_none());
        assert!(parse_navigation_log("not json").is_none());
    }
}
