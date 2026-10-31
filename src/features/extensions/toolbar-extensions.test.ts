import { type Extension } from "@bindings";
import { describe, expect, it } from "vitest";

import { movePin, toolbarExtensions } from "./toolbar-extensions";

const extension = (id: string): Extension => ({
  id,
  name: id,
  version: "1",
  folder: id,
  options: null,
  popup: null,
  icon: null,
});
const installed = [extension("a"), extension("b"), extension("c")];

describe("toolbarExtensions", () => {
  it("lists every extension that is switched on", () => {
    expect(toolbarExtensions(installed, ["b"], []).enabled.map((e) => e.id)).toEqual(["a", "c"]);
  });

  it("shows pins in their order", () => {
    expect(toolbarExtensions(installed, [], ["c", "a"]).pinned.map((e) => e.id)).toEqual(["c", "a"]);
  });

  it("hides the pin of an extension switched off or not installed", () => {
    expect(toolbarExtensions(installed, ["a"], ["a", "gone", "b"]).pinned.map((e) => e.id)).toEqual(["b"]);
  });
});

describe("movePin", () => {
  it("moves a pin left and right among the shown pins", () => {
    expect(movePin(["a", "b", "c"], ["a", "b", "c"], "c", 0)).toEqual(["c", "a", "b"]);
    expect(movePin(["a", "b", "c"], ["a", "b", "c"], "a", 1)).toEqual(["b", "a", "c"]);
    expect(movePin(["a", "b", "c"], ["a", "b", "c"], "a", 2)).toEqual(["b", "c", "a"]);
  });

  it("leaves the pins as they are when nothing moves", () => {
    const pinned = ["a", "b"];
    expect(movePin(pinned, ["a", "b"], "b", 1)).toBe(pinned);
    expect(movePin(pinned, ["a"], "a", 0)).toBe(pinned);
  });

  it("moves nothing when the pins changed since they were shown", () => {
    const pinned = ["a", "b"];
    expect(movePin(pinned, ["a", "b", "c"], "c", 0)).toBe(pinned);
    expect(movePin(pinned, ["a", "c", "b"], "b", 1)).toBe(pinned);
  });

  it("keeps a hidden pin beside the shown pin it followed", () => {
    // `x` is switched off: moving `c` before `b` counts only what shows.
    expect(movePin(["a", "x", "b", "c"], ["a", "b", "c"], "c", 1)).toEqual(["a", "x", "c", "b"]);
    expect(movePin(["a", "b", "x"], ["a", "b"], "a", 1)).toEqual(["b", "a", "x"]);
  });
});
