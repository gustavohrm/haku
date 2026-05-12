export interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Website {
  title: string;
  url: string;
  favicon?: string;
  order: number;
  scrollX?: number;
  scrollY?: number;
}

export interface Tab {
  id: string;
  history: Website[];
  order: number;
}

export interface TabInfoEvent {
  title: string;
  favicon: string;
  url: string;
}

export interface ScrollPositionEvent {
  x: number;
  y: number;
}

export interface ManagedTab {
  id: string;
  history: Website[];
  historyIndex: number;
  readonly current: Website;
  order: number;
}

export interface TabInfoUpdate {
  title: string;
  favicon?: string;
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
  updateSelectedTabScroll(x: number, y: number): void;
  canGoBack(): boolean;
  canGoForward(): boolean;
  goBack(): Website | null;
  goForward(): Website | null;
}
