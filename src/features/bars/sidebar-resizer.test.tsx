import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const updateSettings = vi.fn(async (_change: unknown) => null as unknown);
vi.mock("@ipc/store", () => ({ updateSettings }));

const { SidebarResizer, clampSidebarWidth, MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH } = await import("./sidebar-resizer");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let shell: HTMLDivElement;
let root: Root;

beforeEach(() => {
  shell = document.createElement("div");
  shell.className = "haku-shell";
  document.body.append(shell);
  root = createRoot(shell);
  updateSettings.mockImplementation(async () => ({}));
});

afterEach(() => {
  act(() => root.unmount());
  shell.remove();
  updateSettings.mockReset();
});

function render(side: "left" | "right", width = 240) {
  act(() => root.render(<SidebarResizer side={side} width={width} />));
  return shell.querySelector<HTMLElement>("[role=separator]")!;
}

/** The width the last stored change asked for. */
async function storedWidth() {
  const change = updateSettings.mock.calls.at(-1)?.[0] as (current: object) => { sidebarWidth: number };
  return change({}).sidebarWidth;
}

describe("clampSidebarWidth", () => {
  it("keeps a width within the bounds Rust clamps to", () => {
    expect(clampSidebarWidth(10)).toBe(MIN_SIDEBAR_WIDTH);
    expect(clampSidebarWidth(10_000)).toBe(MAX_SIDEBAR_WIDTH);
    expect(clampSidebarWidth(300.4)).toBe(300);
  });
});

describe("SidebarResizer", () => {
  it("widens a left sidebar with the right arrow, on the shell and in settings", async () => {
    const handle = render("left");

    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));

    expect(shell.style.getPropertyValue("--haku-sidebar-width")).toBe("256px");
    expect(await storedWidth()).toBe(256);
  });

  it("widens a right sidebar with the left arrow", async () => {
    const handle = render("right");

    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true })));

    expect(await storedWidth()).toBe(256);
  });

  it("stops at its bounds", async () => {
    const handle = render("left", MIN_SIDEBAR_WIDTH);

    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true })));
    expect(updateSettings).not.toHaveBeenCalled();

    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
    expect(await storedWidth()).toBe(MAX_SIDEBAR_WIDTH);
  });

  it("goes back to the stored width when storing fails", async () => {
    updateSettings.mockImplementation(async () => null);
    const handle = render("left");

    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));

    expect(shell.style.getPropertyValue("--haku-sidebar-width")).toBe("240px");
  });

  it("restores the default width on a double click", async () => {
    const handle = render("left", 400);

    await act(async () => handle.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));

    expect(await storedWidth()).toBe(240);
  });

  it("steps each key from the last, before the stored width catches up", async () => {
    const handle = render("left");
    const key = () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));

    await act(async () => {
      key();
      key();
    });

    expect(await storedWidth()).toBe(272);
  });

  it("takes focus when dragged, so the keys resize it next", async () => {
    const handle = render("left");
    handle.setPointerCapture = () => {};
    const down = Object.assign(new MouseEvent("pointerdown", { bubbles: true, button: 0 }), {
      pointerId: 1,
      isPrimary: true,
    });

    await act(async () => handle.dispatchEvent(down));

    expect(document.activeElement).toBe(handle);
  });

  it("goes back to where it started when the system cancels a drag", async () => {
    const handle = render("left");
    const pointer = (type: string, clientX: number) =>
      Object.assign(new MouseEvent(type, { bubbles: true, button: 0, clientX }), { pointerId: 1, isPrimary: true });
    handle.setPointerCapture = () => {};

    await act(async () => {
      handle.dispatchEvent(pointer("pointerdown", 100));
      handle.dispatchEvent(pointer("pointermove", 160));
    });
    expect(shell.style.getPropertyValue("--haku-sidebar-width")).toBe("300px");

    await act(async () => handle.dispatchEvent(pointer("pointercancel", 160)));

    expect(shell.style.getPropertyValue("--haku-sidebar-width")).toBe("240px");
    expect(updateSettings).not.toHaveBeenCalled();
  });

  it("starts a drag from where the edge is, before the stored width catches up", async () => {
    const handle = render("left");
    const pointer = (type: string, clientX: number) =>
      Object.assign(new MouseEvent(type, { bubbles: true, button: 0, clientX }), { pointerId: 1, isPrimary: true });
    handle.setPointerCapture = () => {};

    await act(async () => {
      handle.dispatchEvent(pointer("pointerdown", 100));
      handle.dispatchEvent(pointer("pointermove", 160));
      handle.dispatchEvent(pointer("pointerup", 160));
      handle.dispatchEvent(pointer("pointerdown", 160));
      handle.dispatchEvent(pointer("pointermove", 170));
    });

    expect(shell.style.getPropertyValue("--haku-sidebar-width")).toBe("310px");
  });
});
