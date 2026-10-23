import { type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type UpdateSettings } from "./types";

const applyTheme = vi.fn();
vi.mock("@app/theme", () => ({ applyTheme }));

const { KeptSitesControl, ThemeControl } = await import("./controls");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = (keptSites: string[], update: UpdateSettings) => {
  const settings = { keptSites } as Settings;
  act(() =>
    root.render(
      <KeptSitesControl
        settings={settings}
        update={update}
        id="c"
        labelId="l"
        describedBy={undefined}
        disabled={false}
      />,
    ),
  );
};

describe("KeptSitesControl", () => {
  it("says so when no site is kept", () => {
    render([], async () => null);
    expect(container.textContent).toBe("No sites are kept loaded.");
  });

  it("removes only the site whose button was pressed", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    render(["a.test", "b.test"], update);

    const buttons = container.querySelectorAll("button");
    expect([...container.querySelectorAll("li span")].map((span) => span.textContent)).toEqual(["a.test", "b.test"]);
    act(() => buttons[0]?.click());

    const change = update.mock.calls[0]?.[0];
    expect(typeof change).toBe("function");
    const next = (change as (current: Settings) => Settings)({ keptSites: ["a.test", "b.test", "c.test"] } as Settings);
    expect(next.keptSites).toEqual(["b.test", "c.test"]);
  });
});

describe("ThemeControl", () => {
  const renderTheme = (theme: string, update: UpdateSettings) => {
    const settings = { theme } as Settings;
    act(() =>
      root.render(
        <ThemeControl
          settings={settings}
          update={update}
          id="c"
          labelId="l"
          describedBy={undefined}
          disabled={false}
        />,
      ),
    );
  };

  it("offers each theme as a named radio, with the stored one checked", () => {
    renderTheme("dark", async () => null);
    const radios = [...container.querySelectorAll<HTMLInputElement>("input[type=radio]")];

    expect(radios.map((radio) => radio.getAttribute("aria-label"))).toEqual(["Match system", "Light", "Dark"]);
    expect(radios.map((radio) => radio.checked)).toEqual([false, false, true]);
    expect(new Set(radios.map((radio) => radio.name)).size).toBe(1);
  });

  it("applies and stores the theme picked", () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    renderTheme("system", update);
    act(() => container.querySelectorAll<HTMLInputElement>("input[type=radio]")[1]?.click());

    expect(applyTheme).toHaveBeenCalledWith("light");
    expect(update).toHaveBeenCalledWith({ theme: "light" });
  });
});
