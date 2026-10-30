import type { Download } from "@bindings";

/** A download for tests: halfway through a 1 MB file, started this session. */
export const download = (overrides: Partial<Download> = {}): Download => ({
  id: 1,
  url: "https://example.com/report.pdf",
  path: String.raw`C:\Users\me\Downloads\report (1).pdf`,
  received: 512 * 1024,
  total: 1024 * 1024,
  state: "inProgress",
  failure: null,
  canResume: false,
  startedAt: 0,
  recent: true,
  private: false,
  ...overrides,
});
