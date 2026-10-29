import { describe, expect, it } from "vitest";

import {
  MENU_EDGE,
  firstIndex,
  indexByLetter,
  lastIndex,
  placeAtPoint,
  placeBeside,
  stepIndex,
  tidySeparators,
  type MenuItem,
} from "./menu-model";

const command = (id: string, label = id, disabled = false): MenuItem => ({
  kind: "command",
  id,
  label,
  disabled,
  onSelect: () => {},
});
const separator = (id: string): MenuItem => ({ kind: "separator", id });

const window = { width: 800, height: 600 };
const menu = { width: 200, height: 100 };

describe("placeAtPoint", () => {
  it("opens down and to the right of the pointer when it fits", () => {
    expect(placeAtPoint({ x: 100, y: 50 }, menu, window)).toEqual({ x: 100, y: 50 });
  });

  it("flips left when it would leave the window on the right", () => {
    expect(placeAtPoint({ x: 700, y: 50 }, menu, window)).toEqual({ x: 500, y: 50 });
  });

  it("flips up when it would leave the window at the bottom", () => {
    expect(placeAtPoint({ x: 100, y: 550 }, menu, window)).toEqual({ x: 100, y: 450 });
  });

  it("is clamped inside the window when it fits on neither side", () => {
    const tall = { width: 200, height: 500 };
    expect(placeAtPoint({ x: 100, y: 300 }, tall, window)).toEqual({ x: 100, y: MENU_EDGE });
  });

  it("starts at the window's edge when it is larger than the window", () => {
    const huge = { width: 200, height: 900 };
    expect(placeAtPoint({ x: 100, y: 300 }, huge, window).y).toBe(MENU_EDGE);
  });
});

describe("placeBeside", () => {
  const item = { left: 100, top: 40, right: 300, bottom: 70 };

  it("opens to the right of its item, level with its top", () => {
    expect(placeBeside(item, menu, window)).toEqual({ x: 300, y: 40 });
  });

  it("opens to the left of its item when the right has no room", () => {
    const nearEdge = { left: 500, top: 40, right: 700, bottom: 70 };
    expect(placeBeside(nearEdge, menu, window)).toEqual({ x: 300, y: 40 });
  });

  it("rises to end level with its item when it would leave the bottom", () => {
    const low = { left: 100, top: 540, right: 300, bottom: 570 };
    expect(placeBeside(low, menu, window)).toEqual({ x: 300, y: 470 });
  });
});

describe("stepIndex", () => {
  const items = [command("a"), separator("s"), command("b", "b", true), command("c")];

  it("skips separators and disabled items", () => {
    expect(stepIndex(items, 0, 1)).toBe(3);
  });

  it("wraps from the last item to the first", () => {
    expect(stepIndex(items, 3, 1)).toBe(0);
  });

  it("wraps from the first item to the last going up", () => {
    expect(stepIndex(items, 0, -1)).toBe(3);
  });

  it("finds nothing in a menu with nothing choosable", () => {
    expect(stepIndex([separator("s"), command("x", "x", true)], -1, 1)).toBe(-1);
  });

  it("starts from nothing highlighted", () => {
    expect(firstIndex(items)).toBe(0);
    expect(lastIndex(items)).toBe(3);
  });
});

describe("indexByLetter", () => {
  const items = [command("copy", "Copy"), command("cut", "Cut"), command("paste", "Paste")];

  it("jumps to the next item starting with the letter, ignoring case", () => {
    expect(indexByLetter(items, -1, "p")).toBe(2);
  });

  it("moves on to the following match when typed again", () => {
    expect(indexByLetter(items, 0, "c")).toBe(1);
    expect(indexByLetter(items, 1, "c")).toBe(0);
  });

  it("stays where it is when nothing starts with the letter", () => {
    expect(indexByLetter(items, 1, "z")).toBe(1);
  });
});

describe("tidySeparators", () => {
  it("drops separators at either end and doubled ones", () => {
    const items = [separator("1"), command("a"), separator("2"), separator("3"), command("b"), separator("4")];
    expect(tidySeparators(items).map((item) => item.id)).toEqual(["a", "2", "b"]);
  });

  it("leaves an empty menu empty", () => {
    expect(tidySeparators([separator("1"), separator("2")])).toEqual([]);
  });
});
