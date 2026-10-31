import { describe, expect, it } from "vitest";

import { actionsFor, describeStatus, fileName, formatSize, progressShare } from "./describe";
import { download } from "./test-download";

describe("download descriptions", () => {
  it("names a download after the file the engine saved", () => {
    expect(fileName(download())).toBe("report (1).pdf");
  });

  it("reads sizes in the unit they fill", () => {
    expect(formatSize(900)).toBe("900 byte");
    expect(formatSize(1536)).toBe("2 kB");
    expect(formatSize(1024 * 1024 - 1)).toBe("1 MB");
    expect(formatSize(5.25 * 1024 * 1024)).toBe("5.3 MB");
  });

  it("shows progress against the size only when the server gave one", () => {
    expect(describeStatus(download())).toBe("512 kB of 1 MB");
    expect(describeStatus(download({ total: null }))).toBe("512 kB");
    expect(progressShare(download())).toBe(0.5);
    expect(progressShare(download({ total: null }))).toBeNull();
    expect(progressShare(download({ state: "completed" }))).toBeNull();
  });

  it("says why a download failed", () => {
    expect(describeStatus(download({ state: "failed", failure: "network" }))).toBe("Failed — network error");
    expect(describeStatus(download({ state: "failed", failure: "lost" }))).toBe("Interrupted before it finished");
  });

  it.each([
    [download(), ["pause", "cancel"]],
    [download({ state: "paused", canResume: true }), ["resume", "cancel"]],
    [download({ state: "paused" }), ["cancel"]],
    [download({ state: "completed" }), ["open", "show"]],
    [download({ state: "failed", failure: "network", canResume: true }), ["retry", "cancel"]],
    [download({ state: "failed", failure: "lost" }), ["show"]],
    [download({ state: "cancelled" }), []],
  ])("offers only what can be done now (%#)", (listed, actions) => {
    expect(actionsFor(listed)).toEqual(actions);
  });
});
