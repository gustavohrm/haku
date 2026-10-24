import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TabCrash } from "./crash";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const reloadTab = vi.hoisted(() => vi.fn(async () => ({ status: "ok" })));
vi.mock("@ipc/commands", () => ({ commands: { reloadTab } }));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  reloadTab.mockClear();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TabCrash", () => {
  it("says the page crashed and why, without error details", () => {
    act(() => root.render(<TabCrash tabId={3} cause="outOfMemory" />));

    expect(container.querySelector("h1")?.textContent).toBe("This page crashed");
    expect(container.querySelector("p")?.textContent).toBe("It ran out of memory.");
    expect(container.querySelector("details")).toBeNull();
  });

  it("names no reason when the engine gave none", () => {
    act(() => root.render(<TabCrash tabId={3} cause="unknown" />));

    expect(container.querySelector("p")).toBeNull();
  });

  it("reloads the tab when asked", () => {
    act(() => root.render(<TabCrash tabId={3} cause="unknown" />));

    act(() => container.querySelector("button")?.click());

    expect(reloadTab).toHaveBeenCalledWith(3);
  });
});
