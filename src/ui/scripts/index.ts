import type { BrowserBounds } from "../../core/tabs";
import { getCurrentWindow } from "../../shared/tauri";

import { initTabs } from "./tabs";

function getElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);

  if (!element) {
    throw new Error(`Missing required element: ${selector}`);
  }

  return element;
}

function getBrowserBounds(surface: HTMLElement): BrowserBounds {
  const rect = surface.getBoundingClientRect();

  return {
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.max(1, Math.round(rect.width)),
    height: Math.max(1, Math.round(rect.height)),
  };
}

window.addEventListener("DOMContentLoaded", () => {
  const form = getElement<HTMLFormElement>("#url-form");
  const urlInput = getElement<HTMLInputElement>("#url-input");
  const tabStrip = getElement<HTMLDivElement>("#tab-strip");
  const surface = getElement<HTMLDivElement>("#browser-surface");
  const backButton = getElement<HTMLButtonElement>("#back-button");
  const forwardButton = getElement<HTMLButtonElement>("#forward-button");
  const refreshButton = getElement<HTMLButtonElement>("#refresh-button");
  const devtoolsButton = getElement<HTMLButtonElement>("#devtools-button");
  const newTabButton = getElement<HTMLButtonElement>("#new-tab-button");
  const minimizeButton = getElement<HTMLButtonElement>("#minimize-button");
  const maximizeButton = getElement<HTMLButtonElement>("#maximize-button");
  const closeButton = getElement<HTMLButtonElement>("#close-button");
  const appWindow = getCurrentWindow();

  initTabs({
    urlInput,
    form,
    tabStrip,
    surface,
    backButton,
    forwardButton,
    refreshButton,
    devtoolsButton,
    newTabButton,
    getBrowserBounds: () => getBrowserBounds(surface),
  });

  minimizeButton.addEventListener("click", () => {
    void appWindow.minimize();
  });
  maximizeButton.addEventListener("click", () => {
    void appWindow.toggleMaximize();
  });
  closeButton.addEventListener("click", () => {
    void appWindow.close();
  });
});
