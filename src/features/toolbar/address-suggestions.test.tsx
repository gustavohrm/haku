import type { Suggestion, Suggestions, Tab } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { suggest, searchSuggestions, navigateTab, switchToTab, capturePage, shown } = vi.hoisted(() => ({
  suggest: vi.fn(),
  searchSuggestions: vi.fn(),
  navigateTab: vi.fn(),
  switchToTab: vi.fn(),
  capturePage: vi.fn(),
  // The window's tabs, which the backdrop captures.
  shown: { tabs: [] as unknown[] },
}));

vi.mock("@bindings", () => ({
  events: {
    addressFocusRequested: () => ({ listen: async () => () => {} }),
    bookmarkEditRequested: () => ({ listen: async () => () => {} }),
  },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({ commands: { suggest, searchSuggestions, navigateTab, switchToTab, capturePage } }));
vi.mock("@ipc/hooks", () => ({
  useBookmarks: () => null,
  useBrowserState: () => ({
    private: false,
    historyPaused: false,
    viewports: [{ id: "pane-1", tab: 1 }],
    tabs: shown.tabs,
    fullscreen: null,
  }),
  useExtensions: () => ({ installed: [] }),
  useSettings: () => ({
    pinnedExtensions: [],
    disabledExtensions: [],
    keptSites: [],
    historyEnabled: true,
    historyRules: [],
  }),
  useDownloads: () => [],
}));

import { barHoldStore } from "@features/bars/holds";
import { ENGINE_SUGGESTIONS_DELAY_MS } from "@features/search/suggestions";
import { backdropStore } from "@features/viewport/backdrop";

import { Toolbar } from "./toolbar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TAB: Tab = {
  id: 1,
  history: { entries: [{ url: "https://example.com/", title: "Example", favicon: null }], index: 0 },
  presence: { status: "live", slot: 1 },
  scroll: { x: 0, y: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  permission: null,
  audible: false,
  zoom: 100,
  relieved: false,
  restoring: false,
};

const search = (query: string): Suggestion => ({ kind: "search", engine: "DuckDuckGo", query });
const page = (url: string): Suggestion => ({ kind: "page", title: url, url, bookmarked: false });

/** Answers each input with these rows, at once. */
const answering = (rows: (input: string) => Suggestion[], completion: string | null = null) =>
  suggest.mockImplementation(async (input: string) => ({
    status: "ok",
    data: { input, completion, rows: rows(input) } satisfies Suggestions,
  }));

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.clearAllMocks();
  shown.tabs = [TAB];
  searchSuggestions.mockImplementation(async (input: string) => ({ status: "ok", data: { input, rows: [] } }));
  capturePage.mockResolvedValue({ status: "ok", data: "data:image/jpeg;base64,AA==" });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<Toolbar tab={TAB} />));
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

const input = () => container.querySelector<HTMLInputElement>('input[role="combobox"]')!;
const options = () => [...container.querySelectorAll('[role="option"]')];
const selectedText = () => container.querySelector('[role="option"][aria-selected="true"]')?.textContent;

const focus = () => act(async () => input().focus());

const type = async (value: string, inputType = "insertText") => {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    set.call(input(), value);
    input().dispatchEvent(new InputEvent("input", { bubbles: true, inputType }));
  });
};

const press = (key: string) =>
  act(async () => {
    input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });

const enter = () =>
  act(async () => {
    container.querySelector("form")!.requestSubmit();
  });

describe("address suggestions", () => {
  it("lists nothing until the user types", async () => {
    answering(() => [search("x")]);
    await focus();
    expect(suggest).not.toHaveBeenCalled();
    expect(options()).toHaveLength(0);
    expect(input().getAttribute("aria-expanded")).toBe("false");
  });

  it("moves through the rows, putting each address in the field, and opens one as typed", async () => {
    answering((typed) => [search(typed), page("https://a.test/"), page("https://b.test/")]);
    await focus();
    await type("te");

    expect(options()).toHaveLength(3);
    expect(selectedText()).toContain("te");
    await press("ArrowDown");
    await press("ArrowDown");
    await press("ArrowDown");
    expect(input().value).toBe("https://b.test/");
    await press("ArrowUp");
    expect(input().value).toBe("https://a.test/");
    expect(input().getAttribute("aria-activedescendant")).toBe(options()[1]?.id);

    await enter();
    expect(navigateTab).toHaveBeenCalledExactlyOnceWith(1, "https://a.test/", "typed");
  });

  it("restores what was typed on Escape, then leaves the field", async () => {
    answering((typed) => [search(typed), page("https://a.test/")]);
    await focus();
    await type("te");
    await press("ArrowDown");

    await press("Escape");
    expect(input().value).toBe("te");
    expect(document.activeElement).toBe(input());

    await press("Escape");
    expect(document.activeElement).not.toBe(input());
  });

  it("completes a host typed before, with the completed part selected", async () => {
    answering(() => [{ kind: "visit", url: "https://youtube.com" }], "youtube.com");
    await focus();
    await type("YouT");

    expect(input().value).toBe("YouTube.com");
    expect([input().selectionStart, input().selectionEnd]).toEqual([4, 11]);
    await enter();
    expect(navigateTab).toHaveBeenCalledExactlyOnceWith(1, "YouTube.com", "typed");
  });

  it("does not complete again after a deletion, and Escape drops a completion", async () => {
    answering(() => [{ kind: "visit", url: "https://youtube.com" }], "youtube.com");
    await focus();
    await type("you", "deleteContentBackward");
    expect(input().value).toBe("you");

    await type("yout");
    expect(input().value).toBe("youtube.com");
    await press("Escape");
    expect(input().value).toBe("yout");
    expect(document.activeElement).toBe(input());
  });

  it("drops an answer to input it no longer shows", async () => {
    let answerFirst: (() => void) | undefined;
    suggest.mockImplementation(
      (typed: string) =>
        new Promise((resolve) => {
          const answer = () =>
            resolve({ status: "ok", data: { input: typed, completion: null, rows: [search(typed)] } });
          if (typed === "a") {
            answerFirst = answer;
          } else {
            answer();
          }
        }),
    );
    await focus();
    await type("a");
    await type("ab");
    await act(async () => answerFirst?.());

    expect(options().map((option) => option.textContent)).toEqual([expect.stringContaining("ab")]);
  });

  it("switches to an open tab rather than opening it again", async () => {
    answering((typed) => [search(typed), { kind: "tab", tab: 7, title: "Mail", url: "https://mail.test/" }]);
    await focus();
    await type("mail");

    await act(async () => (options()[1] as HTMLElement).click());

    expect(switchToTab).toHaveBeenCalledExactlyOnceWith(7);
    expect(navigateTab).not.toHaveBeenCalled();
  });

  it("puts an engine's keyword in the field, ready for a query", async () => {
    answering((typed) => [search(typed), { kind: "engine", name: "Google", keyword: "@g" }]);
    await focus();
    await type("@");
    await press("ArrowDown");

    await enter();

    expect(input().value).toBe("@g ");
    expect(navigateTab).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(input());
  });
});

describe("engine suggestions", () => {
  const suggested = (query: string, input = query): Suggestion => ({
    kind: "suggested",
    engine: "DuckDuckGo",
    query,
    input,
  });
  const titles = () => options().map((option) => option.querySelector("span")?.textContent);

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("asks the engine once typing pauses, and searches with the completion chosen", async () => {
    answering((typed) => [search(typed)]);
    searchSuggestions.mockImplementation(async (input: string) => ({
      status: "ok",
      data: { input, rows: [suggested(`${input} pages`, `@g ${input} pages`)] },
    }));
    await focus();
    await type("t");
    await type("te");
    expect(searchSuggestions).not.toHaveBeenCalled();

    await act(async () => vi.advanceTimersByTime(ENGINE_SUGGESTIONS_DELAY_MS));
    expect(searchSuggestions).toHaveBeenCalledExactlyOnceWith("te");
    expect(titles()).toEqual(["te", "te pages"]);

    await press("ArrowDown");
    expect(input().value).toBe("@g te pages");
    await enter();
    expect(navigateTab).toHaveBeenCalledExactlyOnceWith(1, "@g te pages", "typed");
  });

  it("lists no completions until the local rows, led by what Enter does, have answered", async () => {
    suggest.mockResolvedValue({ status: "error", error: { kind: "storage", message: "down" } });
    searchSuggestions.mockImplementation(async (input: string) => ({
      status: "ok",
      data: { input, rows: [suggested(`${input} pages`)] },
    }));
    await focus();
    await type("te");
    await act(async () => vi.advanceTimersByTime(ENGINE_SUGGESTIONS_DELAY_MS));

    expect(searchSuggestions).toHaveBeenCalled();
    expect(options()).toHaveLength(0);
  });

  it("makes room for completions from the end of a full list", async () => {
    answering((typed) => [search(typed), ...Array.from({ length: 9 }, (_, index) => page(`https://${index}.test/`))]);
    searchSuggestions.mockImplementation(async (input: string) => ({
      status: "ok",
      data: { input, rows: [suggested("one"), suggested("two")] },
    }));
    await focus();
    await type("te");
    await act(async () => vi.advanceTimersByTime(ENGINE_SUGGESTIONS_DELAY_MS));

    expect(titles()).toHaveLength(10);
    expect(titles().slice(-3)).toEqual(["https://6.test/", "one", "two"]);
  });
});

describe("floating address card", () => {
  const card = () => document.querySelector<HTMLFormElement>("[data-address-card]");
  const field = () => container.querySelector<HTMLInputElement>("input[readonly]")!;
  const editor = () => card()!.querySelector<HTMLInputElement>('input[role="combobox"]')!;
  const open = () => act(async () => field().focus());

  const typeInCard = async (value: string) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(editor(), value);
      editor().dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText" }));
    });
  };

  beforeEach(async () => {
    await act(async () => root.render(<Toolbar tab={TAB} floatingAddress />));
  });

  it("opens on focusing the field, with the address to edit, leaving a floating bar free to hide", async () => {
    expect(card()).toBeNull();

    await open();

    expect(document.activeElement).toBe(editor());
    expect(editor().value).toBe("https://example.com/");
    expect(field().value).toBe("example.com");
    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("closes on Escape and on leaving it, discarding what was typed", async () => {
    await open();
    await typeInCard("abc");
    await act(async () => {
      editor().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(card()).toBeNull();

    await open();
    expect(editor().value).toBe("https://example.com/");
    await act(async () => editor().blur());
    expect(card()).toBeNull();
  });

  it("lists suggestions in the card and closes once one is opened", async () => {
    answering((typed) => [search(typed), page("https://a.test/")]);
    await open();
    await typeInCard("te");
    expect(card()!.querySelectorAll('[role="option"]')).toHaveLength(2);

    await act(async () => card()!.requestSubmit());

    expect(navigateTab).toHaveBeenCalledExactlyOnceWith(1, "te", "typed");
    expect(card()).toBeNull();
  });

  it("puts the page's capture under a backdrop once it is taken, and takes both down on closing", async () => {
    const backdrop = () => document.querySelector("[data-address-backdrop]");
    await open();

    expect(capturePage).toHaveBeenCalledExactlyOnceWith(1);
    expect(backdrop()).not.toBeNull();
    // Later on the page, so drawn over it, though the backdrop arrived last.
    expect(backdrop()!.compareDocumentPosition(card()!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(backdropStore.get()?.get(1)).toBe("data:image/jpeg;base64,AA==");

    await act(async () => editor().blur());
    expect(backdrop()).toBeNull();
    expect(backdropStore.get()).toBeNull();
  });

  it("shows no backdrop when a page cannot be captured", async () => {
    capturePage.mockResolvedValue({ status: "ok", data: null });
    await open();

    expect(card()).not.toBeNull();
    expect(document.querySelector("[data-address-backdrop]")).toBeNull();
    expect(backdropStore.get()).toBeNull();
  });

  it("stays open when pressed outside its input", async () => {
    await open();
    const press = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    await act(async () => card()!.dispatchEvent(press));
    expect(press.defaultPrevented).toBe(true);
    expect(card()).not.toBeNull();
  });
});
