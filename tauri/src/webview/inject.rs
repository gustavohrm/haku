//! Scripts that run inside content webviews, and in extensions' toolbar
//! popups.
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

use crate::model::{Commit, NavigationKind, PageState, Scroll, POPUP_MAX_SIZE};

/// Name of the page-side object that hands over the page's record.
const STATE_READER: &str = "__hakuState";

/// Media shorter than this is not worth keeping a tab for when paused.
const MEDIA_MIN_SECONDS: u32 = 60;

/// How long a restoring page's height must stay unchanged, once it has
/// loaded and grown back to the height it had when it was left, before its
/// scroll is taken as settled.
const SCROLL_QUIET_MS: u32 = 100;

/// The same, for a page that has not grown back to that height, or whose
/// height was not known: content may still be arriving above the restored
/// position, such as late images, and would move what that position shows.
const SCROLL_UNSURE_QUIET_MS: u32 = 1000;

/// Longest a restoring page is held at its scroll before it is taken as
/// settled regardless, for a page that never stops changing height.
pub const SCROLL_SETTLE_LIMIT_MS: u32 = 3000;

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
        .replace("__SCROLL_QUIET_MS__", &SCROLL_QUIET_MS.to_string())
        .replace("__SCROLL_UNSURE_QUIET_MS__", &SCROLL_UNSURE_QUIET_MS.to_string())
        .replace("__SCROLL_SETTLE_LIMIT_MS__", &SCROLL_SETTLE_LIMIT_MS.to_string())
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

  // Whether the scroll handed over by a restore has stopped needing to be
  // re-applied. True when there is none.
  var scrollSettled = true;

  // Holds the page at a restored scroll while it is still growing. A pixel
  // offset only shows the same content once everything above it has its
  // final height, which can be well after `load` on a page whose images
  // arrive late. The height the page had when it was left says when that is.
  // Settled once the page has loaded and stopped changing height, briefly if
  // it is back to that height and for longer if not, or after a limit.
  function holdScroll(x, y, expected) {
    scrollSettled = false;
    var started = Date.now();
    var height = -1;
    var steadySince = started;
    function step() {
      var now = Date.now();
      var current = document.documentElement.scrollHeight;
      if (current !== height) {
        height = current;
        steadySince = now;
      }
      if (window.scrollX !== x || window.scrollY !== y) window.scrollTo(x, y);
      var loaded = document.readyState === "complete";
      var grown = expected > 0 && current >= expected - 2;
      var quiet = grown ? __SCROLL_QUIET_MS__ : __SCROLL_UNSURE_QUIET_MS__;
      if ((loaded && now - steadySince >= quiet) || now - started >= __SCROLL_SETTLE_LIMIT_MS__) {
        scrollSettled = true;
        return;
      }
      setTimeout(step, 50);
    }
    step();
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
          height: document.documentElement.scrollHeight,
          draft: draft(),
        };
      },
      settled: function () {
        return scrollSettled;
      },
      restore: function (state) {
        if (new URL(state.url).origin !== location.origin) return;
        if (state.scroll.x || state.scroll.y) holdScroll(state.scroll.x, state.scroll.y, state.height || 0);
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

/// Expression Rust evaluates to ask whether a restored scroll has settled.
///
/// Evaluates to `true` where the script never ran, since nothing there is
/// being held.
pub fn scroll_settled_expression() -> String {
    format!(r#"typeof window.{STATE_READER} === "object" ? window.{STATE_READER}.settled() : true"#)
}

/// Expression Rust evaluates to end a page's fullscreen, which WebView2 has no
/// call for. A page that is not fullscreen is left alone, and a page that
/// refuses is not an error: its window has already left.
pub const EXIT_FULLSCREEN: &str = "document.fullscreenElement && document.exitFullscreen().catch(() => {})";

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
    height: Option<f64>,
    draft: Option<&'a str>,
}

/// Expression that hands a reloaded page its scroll and draft.
///
/// @param url - The URL they were read on; the page applies them only on the
///   same origin.
/// @param height - The document's height when the scroll was read, if known.
pub fn restore_expression(url: &str, scroll: Scroll, height: Option<f64>, draft: Option<&str>) -> String {
    // JSON is a JavaScript expression, so the values arrive as data, never as
    // code, whatever the page put in its draft.
    let state = serde_json::to_string(&Restore {
        url,
        scroll,
        height,
        draft,
    })
    .unwrap_or_else(|_| "null".into());
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

/// Where an extension page is shown, which decides how its view of windows
/// and tabs is adjusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionView {
    /// In a tab or a popup window, like any page.
    Tab,
    /// As the extension's toolbar popup, opened for the tab the user was on,
    /// whose engine window is given, or none when that tab shows no web page.
    Popup(Option<i64>),
}

/// Name of the flag that marks an extension page as already adjusted.
const EXTENSION_FLAG: &str = "__hakuExtensionPage";

/// How long a popup opened for `chrome.windows.create` is looked for, to hand
/// its window back to the extension.
const FIND_WINDOW_MS: u32 = 3000;

/// How often it is looked for in that time.
const FIND_WINDOW_POLL_MS: u32 = 100;

/// Adjusts what an extension page sees of windows and tabs to how Haku shows
/// them. Does nothing in any page but an extension's own.
///
/// The engine treats every webview as a window of its own, holding one tab,
/// and knows nothing of Haku's windows, so several of the extension APIs do
/// not do what an extension expects:
///
/// - `chrome.windows.create` with `type: "popup"` arrives as a request for a
///   new window without its size, the same as `chrome.tabs.create`, and so
///   opens a tab. It is made through `window.open` with the size and position
///   asked for instead, which opens a popup window, and the extension is handed
///   that window once it appears.
/// - `chrome.windows.update` with a position, size or state moves and
///   resizes the webview inside Haku's window, out of its place. Those are
///   left out, and the rest, such as `focused`, goes through.
/// - `chrome.windows.remove` destroys the page under the tab without the tab
///   closing. It closes the window's tabs with `chrome.tabs.remove` instead,
///   which the tab follows.
/// - `chrome.runtime.openOptionsPage` opens a bare engine window outside
///   Haku. It opens the options page with `chrome.tabs.create` instead, as a
///   tab.
///
/// In the toolbar popup, "the current window", as `currentWindow`,
/// `lastFocusedWindow` and `WINDOW_ID_CURRENT` ask for it, is the window of
/// the tab the user was on, as it is in other browsers, not the popup's own.
/// Without a web page there, the current window has no tabs. And the popup
/// is no tab: `chrome.tabs.getCurrent` answers nothing, as it does in a
/// popup elsewhere, which is how an extension tells its popup from a tab.
///
/// Calls the extension's background worker makes are not adjusted: no script
/// reaches it.
pub fn extension_page_script(view: ExtensionView) -> String {
    let (popup, current) = match view {
        ExtensionView::Tab => (false, None),
        ExtensionView::Popup(current) => (true, current),
    };
    EXTENSION_PAGE_JS
        .replace("__FLAG__", EXTENSION_FLAG)
        .replace("__POPUP__", if popup { "true" } else { "false" })
        .replace(
            "__CURRENT__",
            &current.map_or_else(|| "null".into(), |id| id.to_string()),
        )
        .replace("__FIND_WINDOW_MS__", &FIND_WINDOW_MS.to_string())
        .replace("__FIND_WINDOW_POLL_MS__", &FIND_WINDOW_POLL_MS.to_string())
        .replace("__POPUP_WIDTH__", &super::DEFAULT_POPUP_SIZE.0.to_string())
        .replace("__POPUP_HEIGHT__", &super::DEFAULT_POPUP_SIZE.1.to_string())
        .replace("__POPUP_MAX_WIDTH__", &POPUP_MAX_SIZE.0.to_string())
        .replace("__POPUP_MAX_HEIGHT__", &POPUP_MAX_SIZE.1.to_string())
}

const EXTENSION_PAGE_JS: &str = r#"
(function () {
  if (location.protocol !== "chrome-extension:" || window.__FLAG__) return;
  var api = window.chrome;
  if (!api || !api.tabs || !api.windows || !api.runtime) return;
  Object.defineProperty(window, "__FLAG__", { value: true });

  var POPUP = __POPUP__;
  var CURRENT = __CURRENT__;
  var tabs = api.tabs, windows = api.windows, runtime = api.runtime;
  var WINDOW_ID_CURRENT = windows.WINDOW_ID_CURRENT;
  var query = tabs.query.bind(tabs);
  var removeTabs = tabs.remove.bind(tabs);
  var createTab = tabs.create.bind(tabs);
  var getWindow = windows.get.bind(windows);
  var createWindow = windows.create.bind(windows);
  var updateWindow = windows.update.bind(windows);
  var getCurrent = windows.getCurrent.bind(windows);
  var getLastFocused = windows.getLastFocused.bind(windows);
  var openOptionsPage = runtime.openOptionsPage && runtime.openOptionsPage.bind(runtime);

  // A call with a callback answers through it, and one without through a
  // promise, as the engine's own do.
  function answer(promise, callback) {
    if (typeof callback !== "function") return promise;
    promise.then(function (value) { callback(value); }, function () { callback(); });
  }

  function sleep(ms) {
    return new Promise(function (resolve) { setTimeout(resolve, ms); });
  }

  // The window a popup opened on `href` appeared in, once it does: the one
  // with a tab on it that is not among the tabs open before, `before`.
  function findWindow(href, before) {
    var deadline = Date.now() + __FIND_WINDOW_MS__;
    function look() {
      return query({}).then(function (found) {
        var tab = found.find(function (tab) {
          return before.indexOf(tab.id) === -1 && (tab.url === href || tab.pendingUrl === href);
        });
        if (tab) return getWindow(tab.windowId, { populate: true });
        if (Date.now() > deadline) return undefined;
        return sleep(__FIND_WINDOW_POLL_MS__).then(look);
      });
    }
    return look();
  }

  windows.create = function (data, callback) {
    if (typeof data === "function") return createWindow(data);
    var url = data && (Array.isArray(data.url) ? (data.url.length === 1 ? data.url[0] : null) : data.url);
    if (!data || data.type !== "popup" || typeof url !== "string") return createWindow.apply(null, arguments);
    var href = new URL(url, location.href).href;
    // Without an opener: a window opened on a page of the opener's own origin
    // otherwise keeps the blank first document's window for that page, which
    // is never adjusted, since the extension's APIs do not exist there yet.
    var features = ["popup", "noopener"];
    ["width", "height", "left", "top"].forEach(function (key) {
      if (typeof data[key] === "number") features.push(key + "=" + Math.round(data[key]));
    });
    // Haku takes a request as one for a popup by its size or position, so
    // one asking for neither is given the size a popup gets by default.
    if (features.length === 2) features.push("width=__POPUP_WIDTH__", "height=__POPUP_HEIGHT__");
    var opened = query({}).then(function (before) {
      window.open(href, "_blank", features.join(","));
      return findWindow(href, before.map(function (tab) { return tab.id; }));
    });
    return answer(opened, callback);
  };

  windows.update = function (id, info, callback) {
    var kept = Object.assign({}, info);
    ["left", "top", "width", "height", "state"].forEach(function (key) { delete kept[key]; });
    return updateWindow(id, kept, callback);
  };

  windows.remove = function (id, callback) {
    var target = id === WINDOW_ID_CURRENT ? getCurrent() : Promise.resolve({ id: id });
    var removed = target
      .then(function (target) { return query({ windowId: target.id }); })
      .then(function (found) { return removeTabs(found.map(function (tab) { return tab.id; })); })
      .then(function () {});
    return answer(removed, callback);
  };

  runtime.openOptionsPage = function (callback) {
    var manifest = runtime.getManifest();
    var page = (manifest.options_ui && manifest.options_ui.page) || manifest.options_page;
    if (!page) return openOptionsPage(callback);
    return answer(createTab({ url: runtime.getURL(page) }).then(function () {}), callback);
  };

  if (!POPUP) return;

  // The popup is sized to its page, but only once Haku has measured it, a
  // moment after the page has laid out. A page that reads its window's size
  // as it starts, as Bitwarden does to pick its height, would read the size
  // before that. It reads the size the popup is being made instead, which is
  // what it reads in Chrome, whose popups grow with the page's own layout.
  function fit(name, content, max) {
    var own = Object.getOwnPropertyDescriptor(window, name);
    if (!own || !own.get || !own.configurable) return;
    Object.defineProperty(window, name, {
      configurable: true,
      enumerable: own.enumerable,
      get: function () {
        var real = own.get.call(window), root = document.documentElement;
        return root ? Math.max(real, Math.min(Math.ceil(root[content]), max)) : real;
      },
      set: own.set,
    });
  }
  fit("innerWidth", "scrollWidth", __POPUP_MAX_WIDTH__);
  fit("innerHeight", "scrollHeight", __POPUP_MAX_HEIGHT__);

  function current(getInfo, callback, fallback) {
    if (typeof getInfo === "function") {
      callback = getInfo;
      getInfo = undefined;
    }
    if (CURRENT === null) return fallback(getInfo || {}, callback);
    return answer(getWindow(CURRENT, getInfo || {}), callback);
  }

  tabs.query = function (info, callback) {
    info = Object.assign({}, info);
    if (info.currentWindow === true || info.lastFocusedWindow === true || info.windowId === WINDOW_ID_CURRENT) {
      delete info.currentWindow;
      delete info.lastFocusedWindow;
      if (CURRENT === null) return answer(Promise.resolve([]), callback);
      info.windowId = CURRENT;
    }
    return query(info, callback);
  };
  // A popup is not a tab.
  tabs.getCurrent = function (callback) {
    return answer(Promise.resolve(undefined), callback);
  };
  windows.getCurrent = function (getInfo, callback) {
    return current(getInfo, callback, getCurrent);
  };
  windows.getLastFocused = function (getInfo, callback) {
    return current(getInfo, callback, getLastFocused);
  };
  windows.get = function (id) {
    var args = Array.prototype.slice.call(arguments);
    if (id === WINDOW_ID_CURRENT && CURRENT !== null) args[0] = CURRENT;
    return getWindow.apply(null, args);
  };
})();
"#;

/// Expression Rust evaluates in an extension's toolbar popup to size it to
/// its page: whether the page has loaded, how wide and tall it lays out, and
/// how wide and tall the viewport it lays out in is.
///
/// The popup starts at its smallest, so the page lays out at its narrowest
/// and overflows to the width it needs, as in Chrome, where a popup that sets
/// no width of its own is as narrow as its content allows.
pub fn popup_size_expression() -> &'static str {
    r#"[document.readyState === "complete", document.documentElement.scrollWidth, document.documentElement.scrollHeight, document.documentElement.clientWidth, document.documentElement.clientHeight]"#
}

/// Reads what [`popup_size_expression`] evaluated to.
///
/// @returns Nothing when the answer is not one.
pub fn parse_popup_size(json: &str) -> Option<PopupMeasure> {
    let (loaded, width, height, viewport_width, viewport_height) = serde_json::from_str(json).ok()?;
    Some(PopupMeasure {
        loaded,
        size: (width, height),
        viewport: (viewport_width, viewport_height),
    })
}

/// What [`popup_size_expression`] measured, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupMeasure {
    /// Whether the page has loaded.
    pub loaded: bool,
    /// How wide and tall the page lays out.
    pub size: (f64, f64),
    /// How wide and tall the viewport it lays out in is, without scrollbars.
    pub viewport: (f64, f64),
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
            "__SCROLL_QUIET_MS__",
            "__SCROLL_UNSURE_QUIET_MS__",
            "__SCROLL_SETTLE_LIMIT_MS__",
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
        let expression = restore_expression("https://a.test/", Scroll::default(), None, Some(r#"");alert(1);(""#));

        assert!(expression.contains(r#"\");alert(1);(\""#));
    }

    #[test]
    fn the_extension_page_script_has_every_placeholder_filled() {
        for view in [
            ExtensionView::Tab,
            ExtensionView::Popup(None),
            ExtensionView::Popup(Some(7)),
        ] {
            let script = extension_page_script(view);
            assert!(script.contains(EXTENSION_FLAG));
            for placeholder in [
                "__FLAG__",
                "__POPUP__",
                "__CURRENT__",
                "__FIND_WINDOW_MS__",
                "__FIND_WINDOW_POLL_MS__",
                "__POPUP_WIDTH__",
                "__POPUP_HEIGHT__",
                "__POPUP_MAX_WIDTH__",
                "__POPUP_MAX_HEIGHT__",
            ] {
                assert!(!script.contains(placeholder), "{placeholder} was left in the script");
            }
            assert!(script.contains(r#""width=500", "height=600""#));
        }
    }

    #[test]
    fn the_toolbar_popup_is_told_the_current_window() {
        let script = extension_page_script(ExtensionView::Popup(Some(506_172_816)));
        assert!(script.contains("var POPUP = true;"));
        assert!(script.contains("var CURRENT = 506172816;"));

        let tab = extension_page_script(ExtensionView::Tab);
        assert!(tab.contains("var POPUP = false;"));
        assert!(tab.contains("var CURRENT = null;"));
    }

    #[test]
    fn a_popup_size_is_read_from_its_answer() {
        assert_eq!(
            parse_popup_size("[true,380,600,25,25]"),
            Some(PopupMeasure {
                loaded: true,
                size: (380.0, 600.0),
                viewport: (25.0, 25.0),
            })
        );
        assert_eq!(parse_popup_size("null"), None);
    }

    #[test]
    fn a_page_without_the_log_yields_nothing_so_the_caller_can_fall_back() {
        assert!(parse_navigation_log("null").is_none());
        assert!(parse_navigation_log("not json").is_none());
    }
}
