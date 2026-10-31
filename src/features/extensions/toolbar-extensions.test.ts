import { type Extension } from "@bindings";
import { describe, expect, it } from "vitest";

import { toolbarExtensions } from "./toolbar-extensions";

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

  it("shows pins in the order they were pinned", () => {
    expect(toolbarExtensions(installed, [], ["c", "a"]).pinned.map((e) => e.id)).toEqual(["c", "a"]);
  });

  it("hides the pin of an extension switched off or not installed", () => {
    expect(toolbarExtensions(installed, ["a"], ["a", "gone", "b"]).pinned.map((e) => e.id)).toEqual(["b"]);
  });
});
