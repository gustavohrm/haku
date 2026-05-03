import { invoke } from "@tauri-apps/api/core";

import { createBrowserState, resolveNavigationTarget } from "./browser-state";

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

  async function syncWebviewBounds() {
    await callBrowserCommand("set_browser_bounds", {
      bounds: getBrowserBounds(surface),
    });
  }

  async function loadSelectedTab() {
    urlInput.value = state.selectedTab.url;
    await callBrowserCommand("navigate_browser", { url: state.selectedTab.url });
    renderTabs();
  }

  function renderTabs() {
    tabStrip.replaceChildren();

    for (const tab of state.tabs) {
      const tabButton = document.createElement("button");
      const closeButton = document.createElement("button");
      const isSelected = tab.id === state.selectedTab.id;

      tabButton.className = `tab${isSelected ? " active" : ""}`;
      tabButton.type = "button";
      tabButton.textContent = tab.title;
      tabButton.title = tab.url;
      tabButton.addEventListener("click", async () => {
        state.selectTab(tab.id);
        await loadSelectedTab();
      });

      closeButton.innerHTML = `<i class="ic-close"></i>`;
      closeButton.addEventListener("click", async (event) => {
        event.stopPropagation();
        state.closeTab(tab.id);
        await loadSelectedTab();
      });

      tabButton.append(closeButton);
      tabStrip.append(tabButton);
    }
  }

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
