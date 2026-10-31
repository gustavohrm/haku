import { type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type UpdateSettings } from "./types";

const { BarLayoutControl, KeptSitesControl, TabLayoutControl, ThemeControl } = await import("./controls");

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

  it("stores the theme picked, leaving the shell to apply what was stored", () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    renderTheme("system", update);
    act(() => container.querySelectorAll<HTMLInputElement>("input[type=radio]")[1]?.click());

    expect(update).toHaveBeenCalledWith({ theme: "light" });
  });

  it("keeps the stored theme checked when storing the one picked fails", async () => {
    renderTheme("system", async () => null);
    const radios = [...container.querySelectorAll<HTMLInputElement>("input[type=radio]")];

    await act(async () => radios[1]?.click());

    expect(radios.map((radio) => radio.checked)).toEqual([true, false, false]);
  });
});

describe.each([
  {
    Control: TabLayoutControl,
    field: "tabLayout",
    labels: ["Horizontal", "Vertical"],
    values: ["horizontal", "vertical"],
  },
  { Control: BarLayoutControl, field: "barLayout", labels: ["Docked", "Floating"], values: ["docked", "floating"] },
] as const)("$field control", ({ Control, field, labels, values }) => {
  it("offers each layout as a named radio and stores the one picked", () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    act(() =>
      root.render(
        <Control
          settings={{ [field]: values[0] } as unknown as Settings}
          update={update}
          id="c"
          labelId="l"
          describedBy={undefined}
          disabled={false}
        />,
      ),
    );
    const radios = [...container.querySelectorAll<HTMLInputElement>("input[type=radio]")];

    expect(radios.map((radio) => radio.getAttribute("aria-label"))).toEqual(labels);
    expect(radios.map((radio) => radio.checked)).toEqual([true, false]);
    act(() => radios[1]?.click());
    expect(update).toHaveBeenCalledWith({ [field]: values[1] });
  });
});
