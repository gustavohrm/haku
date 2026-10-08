import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type FocusListener = (event: { payload: number }) => void;

const listeners: FocusListener[] = [];

vi.mock("@bindings", () => ({
  events: {
    addressFocusRequested: {
      listen: vi.fn(async (listener: FocusListener) => {
        listeners.push(listener);
        return () => listeners.splice(listeners.indexOf(listener), 1);
      }),
    },
  },
}));

const { useAddressFocus } = await import("./use-address-focus");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The address field as the toolbar wires it, recording focus changes. */
function Field({ tabId, log }: { tabId: number; log: string[] }) {
  const inputRef = useRef<HTMLInputElement>(null);
  useAddressFocus(tabId, inputRef);
  return <input ref={inputRef} onFocus={() => log.push("focus")} onBlur={() => log.push("blur")} />;
}

describe("useAddressFocus", () => {
  let container: HTMLDivElement;
  let root: Root;
  let log: string[];

  const render = async (tabId: number) => {
    await act(async () => root.render(<Field tabId={tabId} log={log} />));
  };

  const request = async (tabId: number) => {
    await act(async () => listeners.forEach((listener) => listener({ payload: tabId })));
  };

  const input = () => container.querySelector("input") as HTMLInputElement;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    log = [];
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
  });

  it("focuses the field at once for the active tab", async () => {
    await render(1);

    await request(1);

    expect(document.activeElement).toBe(input());
  });

  it("waits for a requested tab to become active, then focuses", async () => {
    await render(1);
    await request(2);
    expect(document.activeElement).not.toBe(input());

    await render(2);

    expect(document.activeElement).toBe(input());
  });

  it("ends an edit begun on the previous tab before focusing for the new one", async () => {
    await render(1);
    act(() => input().focus());
    await request(2);
    log.length = 0;

    await render(2);

    expect(log).toEqual(["blur", "focus"]);
    expect(document.activeElement).toBe(input());
  });

  it("leaves the field when the active tab changes without a request", async () => {
    await render(1);
    act(() => input().focus());

    await render(2);

    expect(document.activeElement).not.toBe(input());
  });
});
