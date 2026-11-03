import { type ListedSearchEngine, type SearchEngine, type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type ControlProps, type UpdateSettings } from "./types";

vi.mock("@features/search/use-search-engines", () => ({ useSearchEngines: vi.fn() }));
vi.mock("@ipc/store", () => ({ tryUpdateSettings: vi.fn() }));

const { useSearchEngines } = await import("@features/search/use-search-engines");
const { tryUpdateSettings } = await import("@ipc/store");
const { SearchEngineControl, SearchEnginesControl } = await import("./search-controls");

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

describe("SearchEngineControl", () => {
  it("offers only the engines that show", () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true, true), listed("mine", false)]);
    render(SearchEngineControl, vi.fn());
    const options = [...container.querySelectorAll("option")].map((option) => option.value);
    expect(options).toEqual(["duckduckgo", "mine"]);
  });
});

describe("SearchEnginesControl", () => {
  it("never hides or removes the default engine", () => {
    engines.mockReturnValue([listed("duckduckgo", true), listed("bing", true)]);
    render(SearchEnginesControl, vi.fn());
    expect(button("Hide engine", 0)?.disabled).toBe(true);
    expect(button("Hide engine", 1)?.disabled).toBe(false);
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

  it("says why an engine whose address cannot search was not added", async () => {
    engines.mockReturnValue([listed("duckduckgo", true)]);
    stored.mockResolvedValue({ status: "ok", data: SETTINGS as Settings });
    render(SearchEnginesControl, vi.fn());

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
    render(SearchEnginesControl, vi.fn());

    await fill("Name", "Wiki");
    await fill("https://example.com/search?q=%s", "https://wiki.test/?q=%s");
    await fill("Keyword", "@d");
    await submit();

    const sent = stored.mock.calls[0]![0](SETTINGS as Settings).customSearchEngines[0];
    expect(sent).toMatchObject({ name: "Wiki", url: "https://wiki.test/?q=%s", keyword: "@d" });
    expect(sent?.id).toMatch(/^custom-/);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("“@d” is already used");
  });
});
