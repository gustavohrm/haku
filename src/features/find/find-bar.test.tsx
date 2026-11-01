import type { Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FIND_QUERY_MAX, FindBar, useFindBar } from "./find-bar";

const { requested, results, ended, listener } = vi.hoisted(() => {
  const listener =
    <T,>(set: Set<(event: { payload: T }) => void>) =>
    () => ({
      listen: async (next: (event: { payload: T }) => void) => {
        set.add(next);
        return () => set.delete(next);
      },
    });
  return {
    requested: new Set<(event: { payload: number }) => void>(),
    ended: new Set<(event: { payload: number }) => void>(),
    results: new Set<(event: { payload: { tab: number; matches: number; active: number | null } }) => void>(),
    listener,
  };
});
vi.mock("@bindings", () => ({
  events: { findRequested: listener(requested), findResult: listener(results), findEnded: listener(ended) },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({ commands: { findInPage: vi.fn(), stopFind: vi.fn() } }));
vi.mock("@features/overlays/use-overlay", () => ({ useOverlay: vi.fn() }));

function tab(id: number, url = "https://a.test/", index = 0): Tab {
  return {
    id,
    history: { entries: [{ url, title: "" }], index },
    presence: { status: "live", slot: 0 },
  } as unknown as Tab;
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.clearAllMocks();
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  requested.clear();
  ended.clear();
  results.clear();
});

const input = () => container.querySelector("input")!;
const type = async (value: string) =>
  act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input(), value);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
const key = async (name: string, shiftKey = false) =>
  act(async () => {
    input().dispatchEvent(new KeyboardEvent("keydown", { key: name, shiftKey, bubbles: true }));
  });
const report = async (payload: { tab: number; matches: number; active: number | null }) =>
  act(async () => results.forEach((next) => next({ payload })));

describe("FindBar", () => {
  it("searches as it is typed in, and moves with Enter and Shift+Enter", async () => {
    await act(async () => root.render(<FindBar tab={3} request={1} onClose={() => {}} />));
    expect(document.activeElement).toBe(input());
    expect(input().maxLength).toBe(FIND_QUERY_MAX);
    await type("cat");
    expect(commands.findInPage).toHaveBeenLastCalledWith(3, "cat", "first");
    await key("Enter");
    expect(commands.findInPage).toHaveBeenLastCalledWith(3, "cat", "next");
    await key("Enter", true);
    expect(commands.findInPage).toHaveBeenLastCalledWith(3, "cat", "previous");
  });

  it("shows its own tab's count, and no results", async () => {
    await act(async () => root.render(<FindBar tab={3} request={1} onClose={() => {}} />));
    await type("cat");
    await report({ tab: 4, matches: 9, active: 1 });
    expect(container.textContent).not.toContain("of");
    await report({ tab: 3, matches: 12, active: 3 });
    expect(container.textContent).toContain("3 of 12");
    await report({ tab: 3, matches: 0, active: null });
    expect(container.textContent).toContain("No results");
  });

  it("clears the search when the field is emptied, and when it closes", async () => {
    await act(async () => root.render(<FindBar tab={3} request={1} onClose={() => {}} />));
    await type("cat");
    await type("");
    expect(commands.stopFind).toHaveBeenCalledWith(3, false);
    vi.mocked(commands.stopFind).mockClear();
    await act(async () => root.render(<></>));
    expect(commands.stopFind).toHaveBeenCalledExactlyOnceWith(3, false);
  });

  it("closes on Escape and gives the page its keyboard back, once", async () => {
    const onClose = vi.fn();
    await act(async () => root.render(<FindBar tab={3} request={1} onClose={onClose} />));
    await key("Escape");
    expect(onClose).toHaveBeenCalledOnce();
    await act(async () => root.render(<></>));
    expect(commands.stopFind).toHaveBeenCalledExactlyOnceWith(3, true);
  });

  it("closes on its close button and gives the page its keyboard back", async () => {
    const onClose = vi.fn();
    await act(async () => root.render(<FindBar tab={3} request={1} onClose={onClose} />));
    await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Close find bar"]')!.click());
    expect(onClose).toHaveBeenCalledOnce();
    expect(commands.stopFind).toHaveBeenCalledExactlyOnceWith(3, true);
  });
});

function Harness({ focused }: { focused: Tab | null }) {
  const { open } = useFindBar(focused);
  return <span data-open={open === null ? "" : `${open.tab}:${open.request}`} />;
}

describe("useFindBar", () => {
  const open = () => container.querySelector("span")!.dataset.open;
  const ask = async (id: number) => act(async () => requested.forEach((next) => next({ payload: id })));

  it("opens for the focused tab, and counts each request", async () => {
    await act(async () => root.render(<Harness focused={tab(3)} />));
    await ask(3);
    expect(open()).toBe("3:1");
    await ask(3);
    expect(open()).toBe("3:2");
  });

  it("closes when another tab takes focus, and stays closed on return", async () => {
    await act(async () => root.render(<Harness focused={tab(3)} />));
    await ask(3);
    await act(async () => root.render(<Harness focused={tab(4)} />));
    expect(open()).toBe("");
    await act(async () => root.render(<Harness focused={tab(3)} />));
    expect(open()).toBe("");
  });

  it("closes when Rust says its page started a navigation, such as a reload", async () => {
    await act(async () => root.render(<Harness focused={tab(3)} />));
    await ask(3);
    await act(async () => ended.forEach((next) => next({ payload: 4 })));
    expect(open()).toBe("3:1");
    await act(async () => ended.forEach((next) => next({ payload: 3 })));
    expect(open()).toBe("");
  });

  it("closes when the page navigates", async () => {
    await act(async () => root.render(<Harness focused={tab(3)} />));
    await ask(3);
    await act(async () => root.render(<Harness focused={tab(3, "https://b.test/")} />));
    expect(open()).toBe("");
  });
});
