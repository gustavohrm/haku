import { describe, expect, it, vi } from "vitest";

import { createStore } from "./store";

describe("createStore", () => {
  it("returns the initial value before anything is set", () => {
    expect(createStore(7).get()).toBe(7);
  });

  it("notifies subscribers when the value changes", () => {
    const store = createStore(0);
    const listener = vi.fn();
    store.subscribe(listener);

    store.set(1);

    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.get()).toBe(1);
  });

  it("stays quiet when set to the value it already holds", () => {
    const store = createStore("a");
    const listener = vi.fn();
    store.subscribe(listener);

    store.set("a");

    expect(listener).not.toHaveBeenCalled();
  });

  it("stops notifying once a subscriber unsubscribes", () => {
    const store = createStore(0);
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    unsubscribe();
    store.set(1);

    expect(listener).not.toHaveBeenCalled();
  });

  it("notifies every subscriber", () => {
    const store = createStore(0);
    const first = vi.fn();
    const second = vi.fn();
    store.subscribe(first);
    store.subscribe(second);

    store.set(1);

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("treats a new object as a change even when its contents match", () => {
    const store = createStore({ value: 1 });
    const listener = vi.fn();
    store.subscribe(listener);

    store.set({ value: 1 });

    expect(listener).toHaveBeenCalledTimes(1);
  });
});
