import { describe, expect, it } from "vitest";

import { createTabState, DEFAULT_TAB_URL, resolveNavigationTarget } from "./state";

describe("resolveNavigationTarget", () => {
  it("keeps direct URLs reachable", () => {
    expect(resolveNavigationTarget("tauri.app")).toBe("https://tauri.app");
    expect(resolveNavigationTarget("http://localhost:1420")).toBe("http://localhost:1420");
  });

  it("turns search text into a search URL", () => {
    expect(resolveNavigationTarget("tauri webview tabs")).toBe(
      "https://www.google.com/search?q=tauri%20webview%20tabs",
    );
  });
});

describe("createTabState", () => {
  it("keeps independent history per logical tab", () => {
    const state = createTabState("https://start.example");

    const docsTabId = state.createTab("https://docs.example");
    state.navigateSelectedTab("https://docs.example/page");
    state.selectTab(state.tabs[0].id);

    expect(state.selectedTab.current.url).toBe("https://start.example");

    state.selectTab(docsTabId);

    expect(state.selectedTab.current.url).toBe("https://docs.example/page");
  });

  it("moves back and forward through TypeScript-managed history", () => {
    const state = createTabState("https://start.example");

    state.navigateSelectedTab("https://start.example/first");
    state.navigateSelectedTab("https://start.example/second");

    expect(state.canGoBack()).toBe(true);
    expect(state.goBack()?.url).toBe("https://start.example/first");
    expect(state.canGoForward()).toBe(true);
    expect(state.goForward()?.url).toBe("https://start.example/second");
  });

  it("drops forward history when navigating after going back", () => {
    const state = createTabState("https://start.example");

    state.navigateSelectedTab("https://start.example/first");
    state.navigateSelectedTab("https://start.example/second");
    state.goBack();
    state.navigateSelectedTab("https://start.example/replacement");

    expect(state.selectedTab.history.map((entry) => entry.url)).toEqual([
      "https://start.example",
      "https://start.example/first",
      "https://start.example/replacement",
    ]);
    expect(state.canGoForward()).toBe(false);
  });

  it("resets the last tab instead of leaving no active tab", () => {
    const state = createTabState("https://start.example");

    state.closeTab(state.selectedTab.id);

    expect(state.tabs).toHaveLength(1);
    expect(state.selectedTab.current.url).toBe(DEFAULT_TAB_URL);
  });

  it("updates title and favicon without replacing history", () => {
    const state = createTabState("https://start.example");

    state.navigateSelectedTab("https://start.example/page");
    state.updateSelectedTabInfo({
      title: "Start Page",
      favicon: "https://start.example/favicon.ico",
      url: "https://start.example/page#loaded",
    });

    expect(state.selectedTab.history).toHaveLength(2);
    expect(state.selectedTab.current).toMatchObject({
      title: "Start Page",
      favicon: "https://start.example/favicon.ico",
      url: "https://start.example/page#loaded",
    });
  });
});
