import {
  createTabState,
  ensureBrowserWebview,
  getCurrentWebsite,
  navigateBrowser,
  openBrowserDevtools,
  reloadBrowser,
  resolveNavigationTarget,
  setBrowserBounds,
} from "../../core/tabs";
import type { BrowserBounds, ManagedTab } from "../../core/tabs";
import { listen } from "../../shared/tauri";

interface TabInfoEvent {
  title: string;
  favicon: string;
  url: string;
}

interface RenderTabsOptions {
  tabs: ManagedTab[];
  selectedTabId: string;
  tabStrip: HTMLElement;
  onSelect(tabId: string): void;
  onClose(tabId: string): void;
}

export interface InitTabsOptions {
  urlInput: HTMLInputElement;
  form: HTMLFormElement;
  tabStrip: HTMLElement;
  surface: HTMLElement;
  backButton: HTMLButtonElement;
  forwardButton: HTMLButtonElement;
  refreshButton: HTMLButtonElement;
  devtoolsButton: HTMLButtonElement;
  newTabButton: HTMLButtonElement;
  getBrowserBounds(): BrowserBounds;
}

function cloneFaviconFallback(): Element {
  const template = document.querySelector<HTMLTemplateElement>("#favicon-fallback-template");
  const fallback = template?.content.firstElementChild?.cloneNode(true);

  if (!(fallback instanceof Element)) {
    throw new Error("Favicon fallback template is missing.");
  }

  return fallback;
}

function createFavicon(tab: ManagedTab): HTMLElement {
  const container = document.createElement("span");
  const fallback = cloneFaviconFallback();

  container.className = "tab-favicon";
  container.append(fallback);

  if (!tab.current.favicon) {
    return container;
  }

  const image = document.createElement("img");
  image.src = tab.current.favicon;
  image.alt = "";
  image.addEventListener("load", () => {
    fallback.toggleAttribute("hidden", true);
  });
  image.addEventListener("error", () => {
    image.remove();
    fallback.toggleAttribute("hidden", false);
  });

  container.append(image);

  return container;
}

function renderTabs({ tabs, selectedTabId, tabStrip, onSelect, onClose }: RenderTabsOptions): void {
  tabStrip.replaceChildren();

  for (const tab of tabs) {
    const tabElement = document.createElement("button");
    const title = document.createElement("span");
    const closeButton = document.createElement("button");

    tabElement.className = tab.id === selectedTabId ? "tab active" : "tab";
    tabElement.type = "button";
    tabElement.title = tab.current.title;
    tabElement.addEventListener("click", () => onSelect(tab.id));

    title.textContent = tab.current.title;

    closeButton.type = "button";
    closeButton.ariaLabel = "Close tab";
    closeButton.innerHTML = '<i class="ic-close"></i>';
    closeButton.addEventListener("click", (event) => {
      event.stopPropagation();
      onClose(tab.id);
    });

    tabElement.append(createFavicon(tab), title, closeButton);
    tabStrip.append(tabElement);
  }
}

function normalizeUrl(url: string): string {
  try {
    return new URL(url).href;
  } catch {
    return url;
  }
}

function shouldApplyTabInfo(eventUrl: string, expectedUrl: string): boolean {
  return normalizeUrl(eventUrl) === normalizeUrl(expectedUrl);
}

function matchesInactiveTabUrl(tabs: ManagedTab[], selectedTabId: string, eventUrl: string): boolean {
  return tabs.some((tab) => tab.id !== selectedTabId && shouldApplyTabInfo(eventUrl, tab.current.url));
}

export function initTabs({
  urlInput,
  form,
  tabStrip,
  surface,
  backButton,
  forwardButton,
  refreshButton,
  devtoolsButton,
  newTabButton,
  getBrowserBounds,
}: InitTabsOptions): void {
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
      onSelect: (tabId) => {
        void selectTab(tabId);
      },
      onClose: (tabId) => {
        void closeTab(tabId);
      },
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

    const url = resolveNavigationTarget(urlInput.value);
    state.navigateSelectedTab(url);
    void showSelectedTab();
  });

  backButton.addEventListener("click", () => {
    const website = state.goBack();

    if (website) {
      syncAddressBar();
      syncTabs();
      pendingNavigationUrl = website.url;
      void navigateBrowser(website.url);
    }
  });

  forwardButton.addEventListener("click", () => {
    const website = state.goForward();

    if (website) {
      syncAddressBar();
      syncTabs();
      pendingNavigationUrl = website.url;
      void navigateBrowser(website.url);
    }
  });

  refreshButton.addEventListener("click", () => {
    void reloadBrowser();
  });
  devtoolsButton.addEventListener("click", () => {
    void openBrowserDevtools();
  });
  newTabButton.addEventListener("click", () => {
    state.createTab();
    void showSelectedTab();
  });

  window.addEventListener("resize", () => {
    void setBrowserBounds(getBrowserBounds());
  });

  void listen<TabInfoEvent>("tab-info", (event) => {
    if (pendingNavigationUrl) {
      if (matchesInactiveTabUrl(state.tabs, state.selectedTab.id, event.payload.url)) {
        return;
      }

      pendingNavigationUrl = null;
    } else if (!shouldApplyTabInfo(event.payload.url, state.selectedTab.current.url)) {
      state.navigateSelectedTab(event.payload.url);
    }

    state.updateSelectedTabInfo(event.payload);
    syncAddressBar();
    syncTabs();
  });

  syncAddressBar();
  syncTabs();
  pendingNavigationUrl = getCurrentWebsite(state.selectedTab).url;
  void ensureBrowserWebview(getCurrentWebsite(state.selectedTab).url, getBrowserBounds());
  surface.querySelector(".surface-placeholder")?.remove();
}
