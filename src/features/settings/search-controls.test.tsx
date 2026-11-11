import { type ListedSearchEngine, type SearchEngine, type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type ControlProps, type UpdateSettings } from "./types";

vi.mock("@features/search/use-search-engines", () => ({ useSearchEngines: vi.fn() }));
vi.mock("@ipc/store", () => ({ tryUpdateSettings: vi.fn() }));

const { useSearchEngines } = await import("@features/search/use-search-engines");
const { tryUpdateSettings } = await import("@ipc/store");
const { SearchEnginesControl } = await import("./search-controls");

const engines = vi.mocked(useSearchEngines);
const stored = vi.mocked(tryUpdateSettings);

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.clearAllMocks();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const engine = (id: string, keyword: string | null = null): SearchEngine => ({
  id,
  name: id,
  url: `https://${id}.test/?q=%s`,
  suggestUrl: null,
  keyword,
});

const listed = (id: string, preset: boolean, hidden = false, keyword: string | null = null): ListedSearchEngine => ({
  engine: engine(id, keyword),
  preset,
  hidden,
});

const SETTINGS: Partial<Settings> = {
  searchEngine: "duckduckgo",
  customSearchEngines: [],
  presetKeywords: {},
  hiddenSearchEngines: [],
};

const render = (Control: (props: ControlProps) => React.ReactNode, update: UpdateSettings, settings = SETTINGS) =>
  act(() =>
    root.render(
      <Control
        settings={settings as Settings}
        update={update}
        id="c"
        labelId="l"
        describedBy={undefined}
        disabled={false}
      />,
    ),
  );

const button = (name: string, row = 0) =>
  [...container.querySelectorAll("li")][row]?.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`);

const buttonByText = (text: string) =>
  [...container.querySelectorAll("button")].find((element) => element.textContent?.trim() === text);

const fill = async (placeholder: string, value: string) => {
  const input = container.querySelector<HTMLInputElement>(`input[placeholder="${placeholder}"]`)!;
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
};

const submit = () =>
  act(async () => {
    container.querySelector("form")!.requestSubmit();
  });

describe("SearchEnginesControl", () => {
  it("marks the default, which can be neither hidden nor removed", () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true)]);
    render(SearchEnginesControl, vi.fn());
    expect(container.querySelectorAll("li")[0]?.textContent).toContain("Default");
    expect(button("Hide engine", 0)).toBeNull();
    expect(button("Hide engine", 1)).not.toBeNull();
  });

  it("makes another engine the default", async () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true)]);
    const update = vi.fn<UpdateSettings>(async () => null);
    render(SearchEnginesControl, update);

    const row = container.querySelectorAll("li")[1]!;
    await act(async () =>
      [...row.querySelectorAll("button")].find((each) => each.textContent === "Make default")!.click(),
    );
    expect(update).toHaveBeenCalledExactlyOnceWith({ searchEngine: "bing" });
  });

  it("hides a preset and removes a custom engine", async () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true), listed("mine", false)]);
    const update = vi.fn<UpdateSettings>(async () => null);
    const current = { ...SETTINGS, customSearchEngines: [engine("mine")] } as Settings;
    render(SearchEnginesControl, update, current);

    await act(async () => button("Hide engine", 1)!.click());
    await act(async () => button("Remove engine", 2)!.click());

    const [hide, remove] = update.mock.calls.map(([change]) => (change as (s: Settings) => Settings)(current));
    expect(hide?.hiddenSearchEngines).toEqual(["bing"]);
    expect(remove?.customSearchEngines).toEqual([]);
  });

  it("folds hidden engines away until asked", async () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true, true)]);
    render(SearchEnginesControl, vi.fn());
    expect(container.querySelectorAll("li")).toHaveLength(1);

    await act(async () => buttonByText("Hidden engines (1)")!.click());
    expect(container.querySelectorAll("li")).toHaveLength(2);
    expect(button("Show engine", 1)).not.toBeNull();
  });

  it("shows no form for adding until it is asked for", async () => {
    engines.mockReturnValue([listed("duckduckgo", true)]);
    render(SearchEnginesControl, vi.fn());
    expect(container.querySelector("form")).toBeNull();

    await act(async () => buttonByText("Add search engine")!.click());
    expect(container.querySelector("form")).not.toBeNull();
  });

  it("edits a preset's keyword in place", async () => {
    engines.mockReturnValue([listed("duckduckgo", true, false, "@d")]);
    stored.mockImplementation(async (change) => ({ status: "ok", data: change(SETTINGS as Settings) }));
    render(SearchEnginesControl, vi.fn());

    await act(async () => button("Edit engine", 0)!.click());
    // A preset's name and address are Haku's.
    expect(container.querySelector('input[placeholder="Name"]')).toBeNull();
    await fill("Keyword", "dd");
    await submit();

    expect(stored.mock.calls[0]![0](SETTINGS as Settings).presetKeywords).toEqual({ duckduckgo: "dd" });
    expect(container.querySelector("form")).toBeNull();
  });

  it("puts a custom engine back when an edit leaves it unable to search", async () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("mine", false)]);
    const current = { ...SETTINGS, customSearchEngines: [engine("mine")] } as Settings;
    stored.mockResolvedValue({ status: "ok", data: { ...current, customSearchEngines: [] } });
    const update = vi.fn<UpdateSettings>(async () => null);
    render(SearchEnginesControl, update, current);

    await act(async () => button("Edit engine", 1)!.click());
    await fill("https://example.com/search?q=%s", "javascript:alert(1)");
    await submit();

    expect(container.querySelector("[role=alert]")?.textContent).toContain("can’t take a search");
    const restored = (update.mock.calls[0]![0] as (s: Settings) => Settings)({ ...current, customSearchEngines: [] });
    expect(restored.customSearchEngines).toEqual([engine("mine")]);
  });

  it("puts the default back too when the engine an edit dropped was the default", async () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("mine", false)]);
    const current = { ...SETTINGS, searchEngine: "mine", customSearchEngines: [engine("mine")] } as Settings;
    // As Rust answers: the engine dropped, and the default moved off it.
    const dropped = { ...current, searchEngine: "duckduckgo", customSearchEngines: [] } as Settings;
    stored.mockImplementation(async (change) => {
      change(current);
      return { status: "ok", data: dropped };
    });
    const update = vi.fn<UpdateSettings>(async () => null);
    render(SearchEnginesControl, update, current);

    await act(async () => button("Edit engine", 1)!.click());
    await fill("https://example.com/search?q=%s", "javascript:alert(1)");
    await submit();

    const restored = (update.mock.calls[0]![0] as (s: Settings) => Settings)(dropped);
    expect(restored.customSearchEngines).toEqual([engine("mine")]);
    expect(restored.searchEngine).toBe("mine");
  });

  it("says why an engine whose address cannot search was not added", async () => {
    engines.mockReturnValue([listed("duckduckgo", true)]);
    stored.mockResolvedValue({ status: "ok", data: SETTINGS as Settings });
    render(SearchEnginesControl, vi.fn());

    await act(async () => buttonByText("Add search engine")!.click());
    await fill("https://example.com/search?q=%s", "javascript:alert(1)");
    await submit();

    expect(container.querySelector("[role=alert]")?.textContent).toContain("can’t take a search");
  });

  it("adds an engine, and says when its keyword was already taken", async () => {
    engines.mockReturnValue([listed("duckduckgo", true, false, "@d")]);
    stored.mockImplementation(async (change) => {
      const next = change(SETTINGS as Settings);
      const added = next.customSearchEngines.map((each) => ({ ...each, keyword: null }));
      return { status: "ok", data: { ...next, customSearchEngines: added } };
    });
    const update = vi.fn<UpdateSettings>(async () => null);
    render(SearchEnginesControl, update);

    await act(async () => buttonByText("Add search engine")!.click());
    await fill("Name", "Wiki");
    await fill("https://example.com/search?q=%s", "https://wiki.test/?q=%s");
    await fill("Keyword", "@d");
    await submit();

    const sent = stored.mock.calls[0]![0](SETTINGS as Settings).customSearchEngines[0];
    expect(sent).toMatchObject({ name: "Wiki", url: "https://wiki.test/?q=%s", keyword: "@d" });
    expect(sent?.id).toMatch(/^custom-/);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("“@d” is already used");
    // Taken back out, so saving again with another keyword adds one engine, not a second.
    const undone = (update.mock.calls[0]![0] as (s: Settings) => Settings)({
      ...SETTINGS,
      customSearchEngines: [sent!],
    } as Settings);
    expect(undone.customSearchEngines).toEqual([]);
    expect(container.querySelector('input[placeholder="Keyword"]')?.getAttribute("aria-invalid")).toBe("true");
    expect(
      container.querySelector('input[placeholder="https://example.com/search?q=%s"]')?.getAttribute("aria-invalid"),
    ).toBe("false");
  });
});
