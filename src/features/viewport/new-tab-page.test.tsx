import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";

const { state } = vi.hoisted(() => ({ state: { private: false } }));
vi.mock("@ipc/hooks", () => ({ useBrowserState: () => state }));

import { NewTabPage } from "./new-tab-page";

it("explains private data retention on the new-tab page", () => {
  state.private = true;
  const page = document.createElement("div");
  page.innerHTML = renderToStaticMarkup(<NewTabPage />);
  expect(page.querySelector("h1")?.textContent).toBe("You're browsing privately");
  expect([...page.querySelectorAll("h2")].map((heading) => heading.textContent)).toEqual([
    "Haku won't keep",
    "Your activity may still be visible to",
  ]);
  expect(page.textContent).toContain("once the last private window closes");
  expect(page.textContent).toContain("Extensions do not run in private windows.");
  expect(page.querySelector("section")?.getAttribute("aria-label")).toBeNull();

  state.private = false;
  const ordinary = renderToStaticMarkup(<NewTabPage />);
  expect(ordinary).toContain("New tab");
  for (const privateOnly of [
    "browsing privately",
    "won't keep",
    "may still be visible",
    "last private window closes",
    "Extensions do not run",
    "ic-hat-glasses",
  ]) {
    expect(ordinary).not.toContain(privateOnly);
  }
});
