import {
  createTabState,
  ensureBrowserWebview,
  navigateBrowser,
  openBrowserDevtools,
  reloadBrowser,
  resolveNavigationTarget,
  setBrowserBounds,
} from "@core/tabs";
import { listen } from "@shared/tauri";
import type { TabInfoEvent, BrowserBounds } from "@shared/types";
import { getElement } from "@ui/scripts/utils";
import { renderTabs } from "./render";

function getBrowserBounds(surface: HTMLElement): BrowserBounds {
  const rect = surface.getBoundingClientRect();

  return {
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.max(1, Math.round(rect.width)),
    height: Math.max(1, Math.round(rect.height)),
  };
}

function urlsMatch(a: string, b: string): boolean {
  try {
    return new URL(a).href === new URL(b).href;
  } catch {
    return a === b;
  }
}

export function initTabs(): void {
  const form = getElement<HTMLFormElement>("#url-form");
  const urlInput = getElement<HTMLInputElement>("#url-input");
  const tabStrip = getElement<HTMLDivElement>("#tab-strip");
  const surface = getElement<HTMLDivElement>("#browser-surface");
  const backButton = getElement<HTMLButtonElement>("#back-button");
  const forwardButton = getElement<HTMLButtonElement>("#forward-button");
  const refreshButton = getElement<HTMLButtonElement>("#refresh-button");
  const devtoolsButton = getElement<HTMLButtonElement>("#devtools-button");
  const newTabButton = getElement<HTMLButtonElement>("#new-tab-button");

  const state = createTabState();
  let pendingNavigationUrl: string | null = null;

  function syncControls(): void {
    backButton.disabled = !state.canGoBack();
    forwardButton.disabled = !state.canGoForward();
  }

  function syncAddressBar(): void {
    urlInput.value = state.selectedTab.current.url;
  }

  function syncTabs(): void {
    renderTabs({
      tabs: state.tabs,
      selectedTabId: state.selectedTab.id,
      tabStrip,
      onSelect: (tabId) => void selectTab(tabId),
      onClose: (tabId) => void closeTab(tabId),
    });
    syncControls();
  }

  async function showSelectedTab(): Promise<void> {
    syncAddressBar();
    syncTabs();
    pendingNavigationUrl = state.selectedTab.current.url;
    await navigateBrowser(state.selectedTab.current.url);
  }

  async function selectTab(tabId: string): Promise<void> {
    state.selectTab(tabId);
    await showSelectedTab();
  }

  async function closeTab(tabId: string): Promise<void> {
    state.closeTab(tabId);
    await showSelectedTab();
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    state.navigateSelectedTab(resolveNavigationTarget(urlInput.value));
    void showSelectedTab();
  });

  backButton.addEventListener("click", () => {
    if (state.goBack()) void showSelectedTab();
  });

  forwardButton.addEventListener("click", () => {
    if (state.goForward()) void showSelectedTab();
  });

  refreshButton.addEventListener("click", () => void reloadBrowser());
  devtoolsButton.addEventListener("click", () => void openBrowserDevtools());

  newTabButton.addEventListener("click", () => {
    state.createTab();
    void showSelectedTab();
  });

  window.addEventListener("resize", () => void setBrowserBounds(getBrowserBounds(surface)));

  void listen<TabInfoEvent>("tab-info", ({ payload }) => {
    if (pendingNavigationUrl) {
      const isFromInactiveTab = state.tabs.some(
        (tab) => tab.id !== state.selectedTab.id && urlsMatch(payload.url, tab.current.url),
      );
      if (isFromInactiveTab) return;
      pendingNavigationUrl = null;
    } else if (!urlsMatch(payload.url, state.selectedTab.current.url)) {
      state.navigateSelectedTab(payload.url);
    }

    state.updateSelectedTabInfo(payload);
    syncAddressBar();
    syncTabs();
  });

  syncAddressBar();
  syncTabs();
  pendingNavigationUrl = state.selectedTab.current.url;
  void ensureBrowserWebview(state.selectedTab.current.url, getBrowserBounds(surface));
  surface.querySelector(".surface-placeholder")?.remove();
}
