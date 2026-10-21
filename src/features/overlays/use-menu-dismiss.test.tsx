import { act, useCallback, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useMenuDismiss } from "./use-menu-dismiss";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A menu opened by its button, with a control outside it to click elsewhere. */
function Menu() {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useMenuDismiss(open, close, menuRef, buttonRef);

  return (
    <>
      <button ref={buttonRef} type="button" data-testid="opener" onClick={() => setOpen(true)}>
        open
      </button>
      <button type="button" data-testid="outside">
        elsewhere
      </button>
      {open && (
        <section ref={menuRef} data-testid="menu">
          <button type="button" data-testid="first">
            first
          </button>
          <button type="button">second</button>
        </section>
      )}
    </>
  );
}

describe("useMenuDismiss", () => {
  let container: HTMLDivElement;
  let root: Root;

  const query = (testId: string) => container.querySelector<HTMLElement>(`[data-testid="${testId}"]`);

  const open = async () => {
    await act(async () => query("opener")?.click());
  };

  const pressEscape = async () => {
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
  };

  const pointerDownOn = async (testId: string) => {
    await act(async () => {
      query(testId)?.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
  };

  beforeEach(async () => {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => root.render(<Menu />));
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
  });

  it("closes on Escape", async () => {
    await open();

    await pressEscape();

    expect(query("menu")).toBeNull();
  });

  it("closes on a pointer down outside the menu", async () => {
    await open();

    await pointerDownOn("outside");

    expect(query("menu")).toBeNull();
  });

  it("stays open on a pointer down inside the menu", async () => {
    await open();

    await pointerDownOn("first");

    expect(query("menu")).not.toBeNull();
  });
});
