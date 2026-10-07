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

use crate::model::{Commit, NavigationKind, PageState, Scroll};

/// Name of the page-side object that hands over the page's record.
const STATE_READER: &str = "__hakuState";

/// Media shorter than this is not worth keeping a tab for when paused.
const MEDIA_MIN_SECONDS: u32 = 60;

/// How long after the first attempt scroll is restored again, for pages that
/// lay out after their own scripts run.
pub const SCROLL_RETRY_MS: u32 = 120;

/// How long after the first attempt a draft is restored again, for pages that
/// build their forms with script.
const DRAFT_RETRY_MS: u32 = 500;

/// Keeps a record of what the page holds that a reload would lose, and puts
/// scroll and form contents back when a discarded tab reloads.
///
/// Nothing is stored in the page. Rust reads the record with
/// [`page_state_drain_expression`] and keeps scroll and drafts itself, so
/// they follow the tab into whichever slot it is restored in.
///
/// - **Unsaved text**: a field or editable region the user typed into, still
///   in the document and not empty, whose form has not been submitted since.
/// - **An armed unload prompt**: a `beforeunload` handler on `window`, once
///   the user has interacted with the page. Listeners are counted by wrapping
///   `addEventListener` and `removeEventListener` on `window` itself.
/// - **Interactions**: trusted clicks and key presses since the URL changed.
/// - **Paused media**: a long `video` or `audio` paused partway through.
/// - **Capture**: a live track from `getUserMedia` or `getDisplayMedia`.
/// - **The draft**: changed form fields, never secret ones: passwords,
///   payment details, one-time codes, hidden and file fields, and anything
///   marked `autocomplete="off"`. A submitted form has no draft.
///
/// Restored values are set through the element's own setter and announced
/// with `input` and `change` events, which is what frameworks that own their
/// fields' state listen for. A restore is only applied on the origin it was
/// read on, so a reload that redirects elsewhere never receives another site's
/// text.
///
/// Runs in the top frame only.
pub fn page_state_script() -> String {
    PAGE_STATE_JS
        .replace("__READER__", STATE_READER)
        .replace("__MEDIA_MIN_SECONDS__", &MEDIA_MIN_SECONDS.to_string())
        .replace("__SCROLL_RETRY_MS__", &SCROLL_RETRY_MS.to_string())
        .replace("__DRAFT_RETRY_MS__", &DRAFT_RETRY_MS.to_string())
}

const PAGE_STATE_JS: &str = r#"
(function () {
  if (window.top !== window || window.__READER__) return;

  var SKIPPED_TYPES = /^(password|hidden|file|submit|button|reset|image)$/i;
  var SECRET_AUTOCOMPLETE = /(^|\s)(off|cc-[a-z-]+|one-time-code|current-password|new-password)(\s|$)/i;
  var SECRET_NAME = /(card.?num|cc.?num|cvv|cvc|csc|iban|security.?code)/i;
  var FIELD_TAGS = /^(INPUT|TEXTAREA|SELECT)$/;

  var addListener = window.addEventListener;
  var removeListener = window.removeEventListener;

  function isSecret(field) {
    if (field.tagName === "INPUT" && SKIPPED_TYPES.test(field.type)) return true;
    var auto = (field.getAttribute("autocomplete") || "") + " " +
      ((field.form && field.form.getAttribute("autocomplete")) || "");
    if (SECRET_AUTOCOMPLETE.test(auto)) return true;
    return SECRET_NAME.test((field.name || "") + " " + (field.id || ""));
  }

  function fields() {
    return Array.prototype.filter.call(
      document.querySelectorAll("input, textarea, select"),
      function (field) { return !isSecret(field); }
    );
  }

  function checkable(field) {
    return field.type === "checkbox" || field.type === "radio";
  }

  function identity(field) {
    return field.tagName + ":" + (field.type || "") + ":" + (field.name || field.id || "");
  }

  // What the user changed in a field, or null when it holds what the page
  // loaded with.
  function changed(field) {
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

  var interactions = 0;
  function count(event) {
    if (event.isTrusted) interactions++;
  }
  document.addEventListener("click", count, true);
  document.addEventListener("keydown", count, true);
  if (window.navigation) {
    navigation.addEventListener("currententrychange", function () { interactions = 0; });
  }

  // Elements the user typed into. Set by a trusted edit; a submit removes its
  // form's fields and holds the draft back until the next edit.
  var edited = [];
  var submitted = false;

  function editable(target) {
    if (!target || target.nodeType !== 1) return null;
    if (FIELD_TAGS.test(target.tagName)) return target;
    if (!target.isContentEditable) return null;
    var host = target;
    while (host.parentElement && host.parentElement.isContentEditable) host = host.parentElement;
    return host;
  }

  document.addEventListener("input", function (event) {
    if (!event.isTrusted) return;
    submitted = false;
    var target = editable(event.target);
    if (target && edited.indexOf(target) === -1) edited.push(target);
  }, true);
  document.addEventListener("submit", function (event) {
    submitted = true;
    var form = event.target;
    edited = edited.filter(function (field) { return field.form !== form && !form.contains(field); });
  }, true);

  function holdsText(element) {
    if (!element.isConnected) return false;
    if (!FIELD_TAGS.test(element.tagName)) return (element.textContent || "").trim() !== "";
    if (isSecret(element)) return false;
    if (checkable(element) || element.tagName === "SELECT") return changed(element) !== null;
    return (element.value || "").trim() !== "";
  }

  var unloadListeners = [];
  window.addEventListener = function (type, listener) {
    if (type === "beforeunload" && listener && unloadListeners.indexOf(listener) === -1) {
      unloadListeners.push(listener);
    }
    return addListener.apply(this, arguments);
  };
  window.removeEventListener = function (type, listener) {
    var index = type === "beforeunload" ? unloadListeners.indexOf(listener) : -1;
    if (index !== -1) unloadListeners.splice(index, 1);
    return removeListener.apply(this, arguments);
  };

  function unloadArmed() {
    var handled = unloadListeners.length > 0 || typeof window.onbeforeunload === "function";
    return handled && !!(navigator.userActivation && navigator.userActivation.hasBeenActive);
  }

  var tracks = [];
  var devices = navigator.mediaDevices;
  ["getUserMedia", "getDisplayMedia"].forEach(function (name) {
    if (!devices || typeof devices[name] !== "function") return;
    var original = devices[name];
    devices[name] = function () {
      return original.apply(this, arguments).then(function (stream) {
        tracks = tracks.filter(live).concat(stream.getTracks());
        return stream;
      });
    };
  });

  function live(track) {
    return track.readyState === "live";
  }

  function mediaPaused() {
    return Array.prototype.some.call(document.querySelectorAll("video, audio"), function (media) {
      return media.paused && !media.ended && media.currentTime > 0 && media.duration > __MEDIA_MIN_SECONDS__;
    });
  }

  function draft() {
    if (submitted) return null;
    var stored = [];
    fields().forEach(function (field, index) {
      var value = changed(field);
      if (value) {
        value.i = index;
        value.k = identity(field);
        stored.push(value);
      }
    });
    return stored.length ? JSON.stringify(stored) : null;
  }

  function set(field, property, value) {
    var prototype = Object.getPrototypeOf(field);
    var descriptor = Object.getOwnPropertyDescriptor(prototype, property);
    if (descriptor && descriptor.set) descriptor.set.call(field, value);
    else field[property] = value;
  }

  var draftRestored = false;
  function restoreDraft(json) {
    if (draftRestored) return;
    var stored = JSON.parse(json);
    var current = fields();
    stored.forEach(function (entry) {
      var field = current[entry.i];
      // The page has changed shape, or already holds something of its own.
      if (!field || identity(field) !== entry.k || changed(field)) return;
      if ("c" in entry) set(field, "checked", entry.c);
      else if ("s" in entry) {
        Array.prototype.forEach.call(field.options, function (option, index) {
          option.selected = !!entry.s[index];
        });
      } else set(field, "value", entry.v);
      field.dispatchEvent(new Event("input", { bubbles: true }));
      field.dispatchEvent(new Event("change", { bubbles: true }));
      draftRestored = true;
    });
  }

  // Tried once the page has loaded, then again after `retry` milliseconds,
  // for pages that lay out or build their forms after their own scripts run.
  function whenLoaded(action, retry) {
    function run() {
      try { action(); } catch (_) {}
      setTimeout(function () { try { action(); } catch (_) {} }, retry);
    }
    if (document.readyState === "complete") run();
    else addListener.call(window, "load", run);
  }

  Object.defineProperty(window, "__READER__", {
    value: {
      read: function () {
        return {
          url: location.href,
          unsaved: edited.some(holdsText),
          unloadArmed: unloadArmed(),
          interactions: interactions,
          mediaPaused: mediaPaused(),
          capturing: tracks.some(live),
          scroll: { x: window.scrollX, y: window.scrollY },
          draft: draft(),
        };
      },
      restore: function (state) {
        if (new URL(state.url).origin !== location.origin) return;
        if (state.scroll.x || state.scroll.y) {
          whenLoaded(function () { window.scrollTo(state.scroll.x, state.scroll.y); }, __SCROLL_RETRY_MS__);
        }
        if (state.draft) {
          whenLoaded(function () { restoreDraft(state.draft); }, __DRAFT_RETRY_MS__);
        }
      },
    },
  });
})();
"#;

/// Expression Rust evaluates to read the record kept by
/// [`page_state_script`].
///
/// Evaluates to `null` where the script never ran, such as an error page or
/// the built-in PDF viewer.
pub fn page_state_drain_expression() -> String {
    format!(r#"typeof window.{STATE_READER} === "object" ? window.{STATE_READER}.read() : null"#)
}

/// Reads what [`page_state_drain_expression`] evaluated to.
///
/// @param json - The evaluation result, as JSON.
/// @returns The reading, or `None` when the page had no record to hand over.
pub fn parse_page_state(json: &str) -> Option<PageState> {
    serde_json::from_str::<Option<PageState>>(json).ok().flatten()
}

#[derive(serde::Serialize)]
struct Restore<'a> {
    url: &'a str,
    scroll: Scroll,
    draft: Option<&'a str>,
}

/// Expression that hands a reloaded page its scroll and draft.
///
/// @param url - The URL they were read on; the page applies them only on the
///   same origin.
pub fn restore_expression(url: &str, scroll: Scroll, draft: Option<&str>) -> String {
    // JSON is a JavaScript expression, so the values arrive as data, never as
    // code, whatever the page put in its draft.
    let state = serde_json::to_string(&Restore { url, scroll, draft }).unwrap_or_else(|_| "null".into());
    format!(r#"typeof window.{STATE_READER} === "object" && {state} && window.{STATE_READER}.restore({state})"#)
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
    fn the_page_state_script_has_every_placeholder_filled() {
        let script = page_state_script();

        assert!(script.contains(STATE_READER));
        for placeholder in [
            "__READER__",
            "__MEDIA_MIN_SECONDS__",
            "__SCROLL_RETRY_MS__",
            "__DRAFT_RETRY_MS__",
        ] {
            assert!(!script.contains(placeholder), "{placeholder} was left in the script");
        }
    }

    #[test]
    fn a_page_reading_is_parsed_from_its_record() {
        let json = r#"{"url":"https://a.test/","unsaved":true,"unloadArmed":false,"interactions":12,
            "mediaPaused":true,"capturing":false,"scroll":{"x":0,"y":480},"draft":"[]"}"#;

        let state = parse_page_state(json).unwrap();

        assert_eq!(state.url, "https://a.test/");
        assert!(state.unsaved);
        assert_eq!(state.interactions, 12);
        assert!(state.media_paused);
        assert_eq!(state.scroll, Scroll { x: 0.0, y: 480.0 });
        assert_eq!(state.draft.as_deref(), Some("[]"));
    }

    #[test]
    fn a_page_without_a_record_reads_as_nothing() {
        assert!(parse_page_state("null").is_none());
        assert!(parse_page_state("not json").is_none());
    }

    #[test]
    fn a_restore_carries_a_draft_as_data_not_code() {
        let expression = restore_expression("https://a.test/", Scroll::default(), Some(r#"");alert(1);(""#));

        assert!(expression.contains(r#"\");alert(1);(\""#));
    }

    #[test]
    fn a_page_without_the_log_yields_nothing_so_the_caller_can_fall_back() {
        assert!(parse_navigation_log("null").is_none());
        assert!(parse_navigation_log("not json").is_none());
    }
}
