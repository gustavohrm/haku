import type { Website } from "../../shared/types";

export interface ManagedTab {
  id: string;
  history: Website[];
  historyIndex: number;
  readonly current: Website;
  order: number;
}

export interface TabInfoUpdate {
  title: string;
  favicon: string;
  url?: string;
}

export interface TabState {
  readonly tabs: ManagedTab[];
  readonly selectedTab: ManagedTab;
  createTab(url?: string): string;
  closeTab(tabId: string): void;
  selectTab(tabId: string): void;
  navigateSelectedTab(url: string): Website;
  updateSelectedTabInfo(info: TabInfoUpdate): void;
  canGoBack(): boolean;
  canGoForward(): boolean;
  goBack(): Website | null;
  goForward(): Website | null;
}

const SEARCH_URL = "https://www.google.com/search?q=";

export const DEFAULT_TAB_URL = "https://www.google.com";

function hasUrlProtocol(value: string): boolean {
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(value);
}

function looksLikeHost(value: string): boolean {
  return value.includes(".") || value.startsWith("localhost") || /^\d{1,3}(\.\d{1,3}){3}(:\d+)?(\/.*)?$/.test(value);
}

function getTitleFromUrl(url: string): string {
  return new URL(url).hostname || url;
}

function createHistoryEntry(url: string, order: number): Website {
  return {
    title: getTitleFromUrl(url),
    url,
    order,
  };
}

function createTab(url: string, order: number): ManagedTab {
  return {
    id: crypto.randomUUID(),
    history: [createHistoryEntry(url, 0)],
    historyIndex: 0,
    get current() {
      return getCurrentWebsite(this);
    },
    order,
  };
}

export function resolveNavigationTarget(input: string): string {
  const value = input.trim();

  if (!value) {
    return DEFAULT_TAB_URL;
  }

  if (hasUrlProtocol(value)) {
    return value;
  }

  if (looksLikeHost(value) && !value.includes(" ")) {
    return `https://${value}`;
  }

  return `${SEARCH_URL}${encodeURIComponent(value)}`;
}

export function getCurrentWebsite(tab: ManagedTab): Website {
  return tab.history[tab.historyIndex];
}

export function createTabState(initialUrl = DEFAULT_TAB_URL): TabState {
  const tabs = [createTab(initialUrl, 0)];
  let selectedTabId = tabs[0].id;

  function getSelectedTab(): ManagedTab {
    const selectedTab = tabs.find((tab) => tab.id === selectedTabId);

    if (!selectedTab) {
      throw new Error("Selected tab no longer exists.");
    }

    return selectedTab;
  }

  return {
    get tabs() {
      return tabs;
    },
    get selectedTab() {
      return getSelectedTab();
    },
    createTab(url = DEFAULT_TAB_URL) {
      const tab = createTab(url, tabs.length);
      tabs.push(tab);
      selectedTabId = tab.id;

      return tab.id;
    },
    closeTab(tabId: string) {
      if (tabs.length === 1) {
        tabs[0].history = [createHistoryEntry(DEFAULT_TAB_URL, 0)];
        tabs[0].historyIndex = 0;
        selectedTabId = tabs[0].id;
        return;
      }

      const tabIndex = tabs.findIndex((tab) => tab.id === tabId);

      if (tabIndex === -1) {
        return;
      }

      tabs.splice(tabIndex, 1);

      if (selectedTabId === tabId) {
        selectedTabId = tabs[Math.max(0, tabIndex - 1)].id;
      }
    },
    selectTab(tabId: string) {
      if (tabs.some((tab) => tab.id === tabId)) {
        selectedTabId = tabId;
      }
    },
    navigateSelectedTab(url: string) {
      const selectedTab = getSelectedTab();
      const nextHistory = selectedTab.history.slice(0, selectedTab.historyIndex + 1);
      const entry = createHistoryEntry(url, nextHistory.length);

      nextHistory.push(entry);
      selectedTab.history = nextHistory;
      selectedTab.historyIndex = nextHistory.length - 1;

      return entry;
    },
    updateSelectedTabInfo({ title, favicon, url }: TabInfoUpdate) {
      const current = getCurrentWebsite(getSelectedTab());

      current.title = title || current.title;
      current.favicon = favicon || undefined;

      if (url) {
        current.url = url;
      }
    },
    canGoBack() {
      return getSelectedTab().historyIndex > 0;
    },
    canGoForward() {
      const selectedTab = getSelectedTab();

      return selectedTab.historyIndex < selectedTab.history.length - 1;
    },
    goBack() {
      if (!this.canGoBack()) {
        return null;
      }

      const selectedTab = getSelectedTab();
      selectedTab.historyIndex -= 1;

      return getCurrentWebsite(selectedTab);
    },
    goForward() {
      if (!this.canGoForward()) {
        return null;
      }

      const selectedTab = getSelectedTab();
      selectedTab.historyIndex += 1;

      return getCurrentWebsite(selectedTab);
    },
  };
}
