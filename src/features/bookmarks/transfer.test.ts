import { beforeEach, describe, expect, it, vi } from "vitest";

const { commands, success } = vi.hoisted(() => ({
  commands: { importBookmarks: vi.fn(), exportBookmarks: vi.fn() },
  success: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ success }) }));

import { exportBookmarks, importBookmarks } from "./transfer";

describe("bookmark import and export", () => {
  beforeEach(() => vi.clearAllMocks());

  it("says how many links and folders an import added, and how many it skipped", async () => {
    commands.importBookmarks.mockResolvedValue({ status: "ok", data: { links: 82, folders: 1, skipped: 4 } });
    await importBookmarks();
    expect(success).toHaveBeenCalledExactlyOnceWith({
      title: "Bookmarks imported",
      description: "82 links and 1 folder. 4 items were skipped.",
    });
  });

  it("says when nothing in the file could be imported", async () => {
    commands.importBookmarks.mockResolvedValue({ status: "ok", data: { links: 0, folders: 0, skipped: 1 } });
    await importBookmarks();
    expect(success).toHaveBeenCalledExactlyOnceWith({
      title: "Bookmarks imported",
      description: "Nothing in the file could be imported. 1 item was skipped.",
    });
  });

  it("says how much an export wrote", async () => {
    commands.exportBookmarks.mockResolvedValue({ status: "ok", data: { links: 1, folders: 34 } });
    await exportBookmarks();
    expect(success).toHaveBeenCalledExactlyOnceWith({
      title: "Bookmarks exported",
      description: "1 link and 34 folders.",
    });
  });

  it("says nothing when the dialog was cancelled or the command failed", async () => {
    commands.importBookmarks.mockResolvedValue({ status: "ok", data: null });
    commands.exportBookmarks.mockResolvedValue({ status: "error", error: { kind: "Unsupported", message: "x" } });
    await importBookmarks();
    await exportBookmarks();
    expect(success).not.toHaveBeenCalled();
  });
});
