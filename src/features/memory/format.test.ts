import { describe, expect, it } from "vitest";

import { formatBytes, formatShare, sumKnown } from "./format";

const MB = 1024 * 1024;

describe("formatBytes", () => {
  it("shows whole megabytes below a gigabyte", () => {
    expect(formatBytes(150.4 * MB)).toBe("150 MB");
  });

  it("shows gigabytes to one decimal from a gigabyte up", () => {
    expect(formatBytes(1536 * MB)).toBe("1.5 GB");
  });

  it("shows a dash for an unknown figure", () => {
    expect(formatBytes(null)).toBe("—");
  });
});

describe("formatShare", () => {
  it("shows a share as a whole percentage", () => {
    expect(formatShare(0.153)).toBe("15%");
  });

  it("shows a dash for an unknown share", () => {
    expect(formatShare(null)).toBe("—");
  });
});

describe("sumKnown", () => {
  it("adds the known figures and skips the unknown ones", () => {
    expect(sumKnown([1, null, 2])).toBe(3);
  });

  it("is unknown when no figure is known", () => {
    expect(sumKnown([null, null])).toBeNull();
  });

  it("is unknown for no figures at all", () => {
    expect(sumKnown([])).toBeNull();
  });
});
