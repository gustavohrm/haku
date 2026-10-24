import { beforeEach, describe, expect, it, vi } from "vitest";

const failure = { kind: "TabNotFound", message: "7" } as const;

vi.mock("@bindings", () => ({
  commands: {
    selectTab: vi.fn(async () => ({ status: "error", error: failure })),
    getState: vi.fn(async () => ({ status: "ok", data: { tabs: [] } })),
    closeTab: vi.fn(async () => ({ status: "error", error: "invalid args `id` for command `close_tab`" })),
    openTab: vi.fn(async () => {
      throw new TypeError("IPC is unavailable");
    }),
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

  it("reports a failure Tauri raised itself as a Tauri error", async () => {
    const result = await commands.closeTab(7);

    const error = { kind: "Tauri", message: "invalid args `id` for command `close_tab`" };
    expect(reported).toHaveBeenCalledWith(error);
    expect(result).toEqual({ status: "error", error });
  });

  it("reports a command that throws and hands it back as a failure", async () => {
    const result = await commands.openTab("https://a.test", false);

    const error = { kind: "Tauri", message: "IPC is unavailable" };
    expect(reported).toHaveBeenCalledWith(error);
    expect(result).toEqual({ status: "error", error });
  });
});
