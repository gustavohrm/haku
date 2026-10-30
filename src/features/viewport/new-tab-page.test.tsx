import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";

const { state } = vi.hoisted(() => ({ state: { private: false } }));
vi.mock("@ipc/hooks", () => ({ useBrowserState: () => state }));

import { NewTabPage } from "./new-tab-page";

it("explains private data retention on the new-tab page", () => {
  state.private = true;
  const html = renderToStaticMarkup(<NewTabPage />);
  expect(html).toContain("No history, session, or reopenable tabs are kept.");
  expect(html).toContain("Extensions do not run.");
  const page = document.createElement("div");
  page.innerHTML = html;
  expect(page.querySelector("section")?.getAttribute("aria-label")).toBeNull();
  expect(page.querySelector("h2")?.textContent).toBe("Private");
  expect(html).toContain("last private window closes");
  state.private = false;
  expect(renderToStaticMarkup(<NewTabPage />)).not.toContain("last private window closes");
});
