import { describe, expect, it } from "vitest";

import { createBrowserState, resolveNavigationTarget } from "./browser-state";

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

describe("createBrowserState", () => {
  it("tracks fake tabs without losing each tab URL", () => {
    const state = createBrowserState("https://start.example");

    const docsTabId = state.createTab("https://docs.example");
    state.updateSelectedTabUrl("https://docs.example/page");
    state.selectTab(state.tabs[0].id);

    expect(state.selectedTab.url).toBe("https://start.example");

    state.selectTab(docsTabId);

    expect(state.selectedTab.url).toBe("https://docs.example/page");
  });
});
