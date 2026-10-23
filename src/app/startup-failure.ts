import { t } from "@shared/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Replaces the splash with a bar saying the interface could not start, a way
 * to reload it or close the window, and the error itself.
 *
 * For a failure before React renders anything, such as a module that throws
 * as it loads, which no error boundary can catch. Without it the splash stays
 * up for good, and the window, having no frame, has no control to close it.
 * Built from the DOM alone and styled by `splash.css`, since React or the
 * interface's stylesheet may be what failed.
 *
 * @param error - What stopped the interface from starting.
 */
export function showStartupFailure(error: unknown): void {
  const bar = document.createElement("header");
  bar.className = "haku-startup-failure";
  bar.dataset.tauriDragRegion = "";

  const message = document.createElement("p");
  message.textContent = t("error.startup");
  bar.append(
    message,
    button(t("error.reloadInterface"), () => globalThis.location.reload()),
    button(t("window.close"), () => void getCurrentWindow().close()),
  );

  const detail = document.createElement("pre");
  detail.className = "haku-startup-failure-detail";
  detail.textContent = error instanceof Error ? (error.stack ?? String(error)) : String(error);

  (document.querySelector("#root") ?? document.body).replaceChildren(bar, detail);
}

function button(label: string, onClick: () => void): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.textContent = label;
  element.addEventListener("click", onClick);
  return element;
}
