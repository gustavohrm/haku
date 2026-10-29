import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Menu, type MenuItem } from "./menu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom lays nothing out, so there is nothing for an overlay to follow.
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    disconnect() {}
  },
);

describe("Menu", () => {
  let container: HTMLDivElement;
  let root: Root;
  let opener: HTMLButtonElement;
  const chosen: string[] = [];
  const onClose = vi.fn();

  const command = (id: string, label: string, disabled = false): MenuItem => ({
    kind: "command",
    id,
    label,
    disabled,
    onSelect: () => chosen.push(id),
  });

  const items: MenuItem[] = [
    command("back", "Back"),
    command("forward", "Forward", true),
    { kind: "separator", id: "s" },
    {
      kind: "submenu",
      id: "more",
      label: "More",
      items: [command("copy", "Copy"), command("cut", "Cut")],
    },
    { kind: "checkbox", id: "keep", label: "Keep loaded", checked: true, onSelect: () => chosen.push("keep") },
  ];

  const focused = () => document.activeElement?.textContent ?? null;
  const press = async (key: string) => {
    await act(async () => {
      (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    });
  };
  const render = async (menuItems: MenuItem[] = items) => {
    await act(async () => {
      root.render(<Menu items={menuItems} at={{ x: 10, y: 10 }} label="Page" onClose={onClose} />);
    });
  };
  const menus = () => document.querySelectorAll('[role="menu"]');

  beforeEach(() => {
    chosen.length = 0;
    onClose.mockReset();
    opener = document.createElement("button");
    document.body.append(opener);
    opener.focus();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    opener.remove();
  });

  it("takes focus on the menu itself when it opens", async () => {
    await render();
    expect(document.activeElement?.getAttribute("role")).toBe("menu");
  });

  it("moves down through enabled items, skipping disabled ones and separators", async () => {
    await render();
    await press("ArrowDown");
    expect(focused()).toBe("Back");
    await press("ArrowDown");
    expect(focused()).toContain("More");
  });

  it("wraps from the last item to the first", async () => {
    await render();
    await press("End");
    await press("ArrowDown");
    expect(focused()).toBe("Back");
  });

  it("jumps to the item starting with a typed letter", async () => {
    await render();
    await press("k");
    expect(focused()).toBe("Keep loaded");
  });

  it("runs the chosen item and closes", async () => {
    await render();
    await press("ArrowDown");
    await press("Enter");
    expect(chosen).toEqual(["back"]);
    expect(onClose).toHaveBeenCalledExactlyOnceWith(true);
  });

  it("does nothing when a disabled item is clicked", async () => {
    await render();
    const disabled = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find(
      (row) => row.textContent === "Forward",
    );
    expect(disabled).toBeDefined();
    await act(async () => disabled?.click());
    expect(chosen).toEqual([]);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("marks a checked checkbox item for assistive technology", async () => {
    await render();
    const row = document.querySelector('[role="menuitemcheckbox"]');
    expect(row?.getAttribute("aria-checked")).toBe("true");
  });

  it("opens a submenu with Right, focused on its first item", async () => {
    await render();
    await press("End");
    await press("ArrowUp");
    await press("ArrowRight");
    expect(menus()).toHaveLength(2);
    expect(focused()).toBe("Copy");
  });

  it("closes only the submenu with Escape, back on its item", async () => {
    await render();
    await press("End");
    await press("ArrowUp");
    await press("ArrowRight");
    await press("Escape");
    expect(menus()).toHaveLength(1);
    expect(focused()).toContain("More");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes the whole menu when a submenu item is chosen", async () => {
    await render();
    await press("End");
    await press("ArrowUp");
    await press("ArrowRight");
    await press("Enter");
    expect(chosen).toEqual(["copy"]);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes with Escape at the top level, from inside", async () => {
    await render();
    await press("Escape");
    expect(onClose).toHaveBeenCalledExactlyOnceWith(true);
  });

  it("closes on a click elsewhere, not from inside", async () => {
    await render();
    await act(async () => {
      opener.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(onClose).toHaveBeenCalledExactlyOnceWith(false);
  });

  it("gives focus back to where it was when it closes from inside", async () => {
    await render();
    await press("ArrowDown");
    await press("Escape");
    await act(async () => root.render(<></>));
    expect(document.activeElement).toBe(opener);
  });

  it("leaves focus alone when it closes because focus went elsewhere", async () => {
    await render();
    await act(async () => {
      window.dispatchEvent(new Event("blur"));
    });
    await act(async () => root.render(<></>));
    expect(document.activeElement).not.toBe(opener);
  });

  it("starts a submenu opened by hover over on its first item when Right opens it", async () => {
    await render();
    const more = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((row) =>
      row.textContent?.includes("More"),
    );
    expect(more).toBeDefined();
    // React builds pointer enter from pointerover.
    await act(async () => more?.dispatchEvent(new MouseEvent("pointerover", { bubbles: true })));
    expect(menus()).toHaveLength(2);
    await press("ArrowRight");
    expect(focused()).toBe("Copy");
  });
});
