import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ErrorBoundary } from "./error-boundary";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let broken = true;

function Page() {
  if (broken) {
    throw new Error("bad row");
  }
  return <p data-testid="page">page</p>;
}

describe("ErrorBoundary", () => {
  let container: HTMLDivElement;
  let root: Root;

  const query = (testId: string) => container.querySelector<HTMLElement>(`[data-testid="${testId}"]`);

  beforeEach(async () => {
    broken = true;
    // React reports every caught render error to the console.
    vi.spyOn(console, "error").mockImplementation(() => {});
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () =>
      root.render(
        <>
          <p data-testid="sibling">sibling</p>
          <ErrorBoundary
            fallback={(error, retry) => (
              <button type="button" data-testid="fallback" onClick={retry}>
                {String(error)}
              </button>
            )}
          >
            <Page />
          </ErrorBoundary>
        </>,
      ),
    );
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });

  it("shows the fallback in place of a child that throws, and keeps the rest", () => {
    expect(query("fallback")?.textContent).toBe("Error: bad row");
    expect(query("sibling")).not.toBeNull();
  });

  it("renders the children again on retry", async () => {
    broken = false;
    await act(async () => query("fallback")?.click());

    expect(query("page")).not.toBeNull();
    expect(query("fallback")).toBeNull();
  });
});
