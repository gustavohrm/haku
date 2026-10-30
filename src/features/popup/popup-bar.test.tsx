import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";

const { state } = vi.hoisted(() => ({ state: { private: false } }));
vi.mock("@ipc/hooks", () => ({ useBrowserState: () => state }));

import { PopupBar } from "./popup-bar";

it("labels private popups even before a page is available", () => {
  state.private = true;
  expect(renderToStaticMarkup(<PopupBar tab={null} />)).toContain("Private");
  state.private = false;
  expect(renderToStaticMarkup(<PopupBar tab={null} />)).not.toContain("Private");
});
