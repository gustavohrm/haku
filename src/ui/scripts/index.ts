import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { createBrowserState, resolveNavigationTarget } from "./browser-state";
import tabManager from "./tabs";

interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

const state = createBrowserState("https://www.google.com");

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

async function callBrowserCommand(command: string, args = {}) {
  await invoke(command, args);
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

  tabManager.init(tabStrip);

  async function syncWebviewBounds() {
    await callBrowserCommand("set_browser_bounds", {
      bounds: getBrowserBounds(surface),
    });
  }

  function renderTabs() {
    tabManager.render(
      state.tabs,
      state.selectedTab.id,
      async (id) => {
        state.selectTab(id);
        await loadSelectedTab();
      },
      async (id) => {
        state.closeTab(id);
        await loadSelectedTab();
      },
    );
  }

  async function loadSelectedTab() {
    urlInput.value = state.selectedTab.url;
    await callBrowserCommand("navigate_browser", { url: state.selectedTab.url });
    renderTabs();
  }

  listen<{ title: string; favicon: string; url: string }>("tab-info", (event) => {
    const { title, favicon, url } = event.payload;
    state.updateTabInfo(state.selectedTab.id, title, favicon, url);
    urlInput.value = url;
    renderTabs();
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();

    const url = resolveNavigationTarget(urlInput.value);
    state.updateSelectedTabUrl(url);
    await loadSelectedTab();
  });

  backButton.addEventListener("click", () => callBrowserCommand("browser_go_back"));
  forwardButton.addEventListener("click", () => callBrowserCommand("browser_go_forward"));
  refreshButton.addEventListener("click", () => callBrowserCommand("reload_browser"));
  devtoolsButton.addEventListener("click", () => callBrowserCommand("open_browser_devtools"));
  minimizeButton.addEventListener("click", () => appWindow.minimize());
  maximizeButton.addEventListener("click", () => appWindow.toggleMaximize());
  closeButton.addEventListener("click", () => appWindow.close());
  newTabButton.addEventListener("click", async () => {
    state.createTab();
    await loadSelectedTab();
  });

  window.addEventListener("resize", () => {
    void syncWebviewBounds();
  });

  urlInput.value = state.selectedTab.url;
  renderTabs();
  void callBrowserCommand("ensure_browser_webview", {
    url: state.selectedTab.url,
    bounds: getBrowserBounds(surface),
  });
});
