export interface BrowserTab {
  id: string;
  title: string;
  url: string;
}

export interface BrowserState {
  readonly tabs: BrowserTab[];
  readonly selectedTab: BrowserTab;
  createTab(url?: string): string;
  closeTab(tabId: string): void;
  selectTab(tabId: string): void;
  updateSelectedTabUrl(url: string): void;
}

const SEARCH_URL = "https://www.google.com/search?q=";
const DEFAULT_URL = "https://www.google.com";

function hasUrlProtocol(value: string): boolean {
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(value);
}

function looksLikeHost(value: string): boolean {
  return value.includes(".") || value.startsWith("localhost") || /^\d{1,3}(\.\d{1,3}){3}(:\d+)?(\/.*)?$/.test(value);
}

export function resolveNavigationTarget(input: string): string {
  const value = input.trim();

  if (!value) {
    return DEFAULT_URL;
  }

  if (hasUrlProtocol(value)) {
    return value;
  }

  if (looksLikeHost(value) && !value.includes(" ")) {
    return `https://${value}`;
  }

  return `${SEARCH_URL}${encodeURIComponent(value)}`;
}

function createTab(url: string, count: number): BrowserTab {
  return {
    id: crypto.randomUUID(),
    title: `Tab ${count}`,
    url,
  };
}

export function createBrowserState(initialUrl = DEFAULT_URL): BrowserState {
  const tabs = [createTab(initialUrl, 1)];
  let selectedTabId = tabs[0].id;

  return {
    get tabs() {
      return tabs;
    },
    get selectedTab() {
      const selectedTab = tabs.find((tab) => tab.id === selectedTabId);

      if (!selectedTab) {
        throw new Error("Selected tab no longer exists.");
      }

      return selectedTab;
    },
    createTab(url = DEFAULT_URL) {
      const tab = createTab(url, tabs.length + 1);
      tabs.push(tab);
      selectedTabId = tab.id;

      return tab.id;
    },
    closeTab(tabId: string) {
      if (tabs.length === 1) {
        tabs[0].url = DEFAULT_URL;
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
    updateSelectedTabUrl(url: string) {
      this.selectedTab.url = url;
      this.selectedTab.title = new URL(url).hostname || url;
    },
  };
}
