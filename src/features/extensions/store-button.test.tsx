import { type ExtensionToAdd } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { commands, success } = vi.hoisted(() => ({
  commands: { addExtensionFromStore: vi.fn(), confirmExtension: vi.fn() },
  success: vi.fn(),
}));

vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ success }), reportUnexpected: vi.fn() }));

import { StoreButton } from "./store-button";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ID = "ddkjiahejlhfcafbddmgiahcphecmpfh";
const TO_ADD: ExtensionToAdd = {
  token: 7,
  name: "Blocker",
  version: "2.0",
  icon: null,
  replaces: null,
  warnings: [{ access: { kind: "allSites" }, new: true }],
};

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  commands.addExtensionFromStore.mockResolvedValue({ status: "ok", data: TO_ADD });
  commands.confirmExtension.mockResolvedValue({ status: "ok", data: null });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<StoreButton id={ID} />));
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

const button = () => host.querySelector("button")!;
const question = () => document.body.querySelector<HTMLElement>('[role="alert"]');

async function add() {
  await act(async () => button().click());
}

it("downloads the extension the page is about and asks under the button", async () => {
  await add();
  expect(commands.addExtensionFromStore).toHaveBeenCalledWith(ID);
  expect(question()?.textContent).toContain("Add Blocker 2.0?");
  expect(button().getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement?.textContent).toBe("Cancel");

  const install = [...question()!.querySelectorAll("button")].find((each) => each.textContent === "Add")!;
  await act(async () => install.click());
  expect(commands.confirmExtension).toHaveBeenCalledWith(7, true);
  expect(question()).toBeNull();
});

it("answers no when the question is closed with Escape", async () => {
  await add();
  await act(async () =>
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })),
  );
  expect(commands.confirmExtension).toHaveBeenCalledWith(7, false);
  expect(question()).toBeNull();
  expect(document.activeElement).toBe(button());
});

it("answers no when the tab leaves the page while it asks", async () => {
  await add();
  await act(async () => root.render(<></>));
  expect(commands.confirmExtension).toHaveBeenCalledWith(7, false);
});

it("answers no to a download that arrives after the tab left the page", async () => {
  let finish: (value: unknown) => void = () => {};
  commands.addExtensionFromStore.mockReturnValue(new Promise((resolve) => (finish = resolve)));
  await act(async () => button().click());
  await act(async () => root.render(<></>));
  await act(async () => finish({ status: "ok", data: TO_ADD }));
  expect(commands.confirmExtension).toHaveBeenCalledWith(7, false);
  await act(async () => root.render(<StoreButton id={ID} />));
  expect(question()).toBeNull();
  expect(button().disabled).toBe(false);
});

it("asks nothing when the download fails", async () => {
  commands.addExtensionFromStore.mockResolvedValue({ status: "error", error: { kind: "network" } });
  await add();
  expect(question()).toBeNull();
  expect(button().disabled).toBe(false);
});
