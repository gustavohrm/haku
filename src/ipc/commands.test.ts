import { beforeEach, describe, expect, it, vi } from "vitest";

const failure = { kind: "TabNotFound", message: "7" } as const;

vi.mock("@bindings", () => ({
  commands: {
    selectTab: vi.fn(async () => ({ status: "error", error: failure })),
    getState: vi.fn(async () => ({ status: "ok", data: { tabs: [] } })),
  },
}));

const { commands, onCommandFailure } = await import("./commands");

describe("commands", () => {
  const reported = vi.fn();

  beforeEach(() => {
    reported.mockReset();
    onCommandFailure(reported);
  });

  it("reports a failure and still hands the result back to the caller", async () => {
    const result = await commands.selectTab(7);

    expect(reported).toHaveBeenCalledWith(failure);
    expect(result).toEqual({ status: "error", error: failure });
  });

  it("reports nothing when the command succeeds", async () => {
    await commands.getState();

    expect(reported).not.toHaveBeenCalled();
  });
});
