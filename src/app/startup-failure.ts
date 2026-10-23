import { crashStyle, describeCrash } from "@app/crash-style";
import { windowButtonClass } from "@features/window-controls/window-button-class";
import { t } from "@shared/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Replaces the splash with the window's frame and a notice that the interface
 * could not start, with a way to reload it, the window controls, and the
 * error.
 *
 * For a failure before React renders anything, such as a module that throws
 * as it loads, which no error boundary can catch. Without it the splash stays
 * up for good, and the window, having no frame, has no control to close it.
 * Built without React, since React may be what failed, from the same classes
 * as the interface's own crash screen; the stylesheet is loaded before the
 * interface is.
 *
 * @param error - What stopped the interface from starting.
 */
export function showStartupFailure(error: unknown): void {
  const bar = element(
    "header",
    crashStyle.bar,
    element(
      "div",
      crashStyle.controls,
      windowButton("ic-minus", t("window.minimize"), () => void getCurrentWindow().minimize()),
      windowButton("ic-square", t("window.maximize"), () => void getCurrentWindow().toggleMaximize()),
      windowButton("ic-x", t("window.close"), () => void getCurrentWindow().close(), true),
    ),
  );
  bar.dataset.tauriDragRegion = "";

  const reload = element("button", crashStyle.action, t("error.reloadInterface"));
  reload.type = "button";
  reload.addEventListener("click", () => globalThis.location.reload());

  const mark = element("i", crashStyle.mark);
  mark.setAttribute("aria-hidden", "true");

  const notice = element(
    "div",
    crashStyle.notice,
    mark,
    element("h1", crashStyle.title, t("error.startup")),
    element("p", crashStyle.description, t("error.startup.help")),
    reload,
    element(
      "details",
      crashStyle.details,
      element("summary", crashStyle.summary, t("error.details")),
      element("pre", crashStyle.error, describeCrash(error)),
    ),
  );

  const surface = element("main", crashStyle.surface, element("div", crashStyle.page, notice));
  const frame = element("div", crashStyle.frame, bar, surface);
  frame.dataset.density = "compact";
  (document.querySelector("#root") ?? document.body).replaceChildren(frame);
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  node.append(...children);
  return node;
}

function windowButton(icon: string, label: string, onClick: () => void, danger = false): HTMLButtonElement {
  const glyph = element("i", `${icon} ic-sm`);
  glyph.setAttribute("aria-hidden", "true");
  const button = element("button", windowButtonClass(danger), glyph);
  button.type = "button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.addEventListener("click", onClick);
  return button;
}
