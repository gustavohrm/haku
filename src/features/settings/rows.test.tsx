import { type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { SettingRow } from "./rows";
import { type ControlProps, type SettingRowDefinition } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const settings = { discardTabs: "smart" } as Settings;

function Input({ id, describedBy, disabled }: ControlProps) {
  return <input id={id} aria-describedby={describedBy} disabled={disabled} />;
}

const row: SettingRowDefinition = {
  id: "capacity",
  title: "settings.capacity",
  description: "settings.capacity.help",
  disabledReason: (current) => (current.discardTabs === "always" ? "settings.disabledByDiscard" : null),
  Control: Input,
};

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

const render = (current: Settings) =>
  act(() => root.render(<SettingRow row={row} context={{ settings: current, update: async () => null }} />));

describe("SettingRow", () => {
  it("labels and describes its control", () => {
    render(settings);
    const input = container.querySelector("input");
    const label = container.querySelector("label");
    const description = document.getElementById(input?.getAttribute("aria-describedby") ?? "");

    expect(label?.htmlFor).toBe(input?.id);
    expect(label?.textContent).toBe("Loaded tabs");
    expect(description?.textContent).toMatch(/^How many tabs/);
    expect(input?.disabled).toBe(false);
  });

  it("says why it is disabled in place of its description", () => {
    render({ ...settings, discardTabs: "always" });
    const input = container.querySelector("input");
    const description = document.getElementById(input?.getAttribute("aria-describedby") ?? "");

    expect(input?.disabled).toBe(true);
    expect(description?.textContent).toBe("Has no effect while background tabs are always discarded.");
  });
});
