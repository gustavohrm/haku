import type { Tab } from "@bindings";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { useActiveTab } from "./hooks";
import { browserStore, EMPTY_STATE } from "./store";

vi.mock("@bindings", () => ({ events: {} }));
vi.mock("./commands", () => ({ commands: {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("derives the active tab from the focused viewport assignment", async () => {
  const first = { id: 1 } as Tab;
  const second = { id: 2 } as Tab;
  browserStore.set({
    ...EMPTY_STATE,
    tabs: [first, second],
    active: first.id,
    viewports: [{ id: "pane", tab: second.id }],
    focusedViewport: "pane",
  });
  function Active() {
    return <span>{useActiveTab()?.id ?? "none"}</span>;
  }
  const container = document.createElement("div");
  const root = createRoot(container);
  try {
    await act(async () => root.render(<Active />));
    expect(container.textContent).toBe("2");
    await act(async () => browserStore.set(EMPTY_STATE));
    expect(container.textContent).toBe("none");
  } finally {
    await act(async () => root.unmount());
    browserStore.set(EMPTY_STATE);
  }
});
