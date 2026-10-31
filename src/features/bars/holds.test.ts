import { afterEach, describe, expect, it } from "vitest";

import { barHoldStore, holdBar as hold, holdBarForWindow as holdForWindow, type BarContent } from "./holds";

const releases: (() => void)[] = [];
const holdBar = (content: BarContent) => {
  const release = hold(content);
  releases.push(release);
  return release;
};
const holdBarForWindow = (content: BarContent) => {
  const release = holdForWindow(content);
  releases.push(release);
  return release;
};

afterEach(() => {
  releases.splice(0).forEach((release) => release());
  barHoldStore.set({ tabs: 0, toolbar: 0 });
});

describe("holdBar", () => {
  it("counts each hold until it is released", () => {
    const first = holdBar("tabs");
    const second = holdBar("tabs");
    expect(barHoldStore.get().tabs).toBe(2);

    first();
    expect(barHoldStore.get().tabs).toBe(1);
    second();
    expect(barHoldStore.get().tabs).toBe(0);
  });

  it("releases only once however often it is called", () => {
    const release = holdBar("toolbar");
    holdBar("toolbar");

    release();
    release();

    expect(barHoldStore.get().toolbar).toBe(1);
  });

  it("leaves the other content alone", () => {
    holdBar("toolbar");
    expect(barHoldStore.get().tabs).toBe(0);
  });
});

describe("holdBarForWindow", () => {
  it("holds while the chrome has not lost focus, whatever the pointer does", () => {
    holdBarForWindow("toolbar");

    document.dispatchEvent(new Event("pointermove"));

    expect(barHoldStore.get().toolbar).toBe(1);
  });

  it("ends when the pointer returns after the window took focus", () => {
    holdBarForWindow("toolbar");

    window.dispatchEvent(new Event("blur"));
    expect(barHoldStore.get().toolbar).toBe(1);
    document.dispatchEvent(new Event("pointermove"));

    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("ends when focus returns after the window took it", () => {
    holdBarForWindow("toolbar");

    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));

    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("ends on a click in the chrome", () => {
    holdBarForWindow("toolbar");

    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));

    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("ends early when released, and stops listening", () => {
    const release = holdBarForWindow("toolbar");
    release();
    holdBar("toolbar");

    window.dispatchEvent(new Event("blur"));
    document.dispatchEvent(new Event("pointermove"));

    expect(barHoldStore.get().toolbar).toBe(1);
  });
});
