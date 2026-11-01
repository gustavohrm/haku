import type { ContextMenuRequested, ContextMenuTarget, KeyBinding, NativeMenuItem, Shortcut, Tab } from "@bindings";
import type { MenuItem } from "@features/overlays/menu-model";
import { keymapStore } from "@ipc/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { fieldMenu, interfaceMenu, pageMenu, stripMenu, tabMenu, type PageActions } from "./items";

// The defaults these menus show hints for, as Rust reports them.
const DEFAULT_HINTS: [Shortcut, string][] = [
  ["newTab", "Ctrl+T"],
  ["reopenClosedTab", "Ctrl+Shift+T"],
  ["closeTab", "Ctrl+W"],
  ["reload", "Ctrl+R"],
  ["duplicateTab", "Ctrl+Shift+K"],
  ["toggleSplit", "Alt+Shift+N"],
  ["back", "Alt+Left"],
  ["forward", "Alt+Right"],
  ["devTools", "F12"],
  ["print", "Ctrl+P"],
  ["savePage", "Ctrl+S"],
];

function keymap(changed: Partial<Record<Shortcut, string[]>> = {}): KeyBinding[] {
  return DEFAULT_HINTS.map(([shortcut, chord]) => ({
    shortcut,
    chords: changed[shortcut] ?? [chord],
    defaults: [chord],
  }));
}

beforeEach(() => keymapStore.set(keymap()));

function tab(id: number, urls: string[] = ["https://example.com/"], index = urls.length - 1): Tab {
  return {
    id,
    history: { entries: urls.map((url) => ({ url, title: "", favicon: null })), index },
    fixed: false,
    activeAt: 0,
    dialog: null,
    permission: null,
    audible: false,
    relieved: false,
    restoring: false,
  } as unknown as Tab;
}

function native(id: number, name: string, extra: Partial<NativeMenuItem> = {}): NativeMenuItem {
  return {
    id,
    name,
    label: name,
    kind: "command",
    enabled: true,
    checked: false,
    shortcut: null,
    children: [],
    ...extra,
  };
}

function request(target: Partial<ContextMenuTarget>, items: NativeMenuItem[] = []): ContextMenuRequested {
  return {
    request: 1,
    tab: 1,
    x: 0,
    y: 0,
    target: {
      kind: "page",
      pageUrl: "https://example.com/",
      linkUrl: null,
      linkText: null,
      sourceUrl: null,
      selection: null,
      editable: false,
      ...target,
    },
    items,
  };
}

function actions(): PageActions {
  return {
    isPrivate: false,
    back: vi.fn(),
    forward: vi.fn(),
    reload: vi.fn(),
    print: vi.fn(),
    savePage: vi.fn(),
    inspect: vi.fn(),
    openInTab: vi.fn(),
    openInWindow: vi.fn(),
    openInPrivateWindow: vi.fn(),
    search: vi.fn(),
    choose: vi.fn(),
  };
}

const ids = (items: MenuItem[]) => items.map((item) => item.id);
const find = (items: MenuItem[], id: string) => items.find((item) => item.id === id);
const select = (item: MenuItem | undefined) => {
  if (item && "onSelect" in item) {
    item.onSelect();
  }
};

describe("tabMenu", () => {
  it("shows matching shortcut hints without advertising split creation on an existing partner", () => {
    const tabs = [tab(1), tab(2)];
    const items = tabMenu(tabs[0]!, tabs, tabActions(), { active: 1, splits: [{ tabs: [1, 2], ratio: 0.5 }] });
    for (const [id, shortcut] of [
      ["reload", "Ctrl+R"],
      ["duplicate", "Ctrl+Shift+K"],
      ["separate", "Alt+Shift+N"],
      ["close", "Ctrl+W"],
    ]) {
      expect(find(items, id!)).toMatchObject({ shortcut });
    }
    expect(find(items, "split")).not.toHaveProperty("shortcut");
    const background = tabMenu(tabs[1]!, [...tabs, tab(3)], tabActions(), {
      active: 3,
      splits: [{ tabs: [1, 2], ratio: 0.5 }],
    });
    for (const id of ["reload", "duplicate", "separate", "close"]) {
      expect(find(background, id)).toMatchObject({ shortcut: null });
    }
    const strip = stripMenu({ newTab: vi.fn(), reopen: vi.fn() });
    expect(find(strip, "new")).toMatchObject({ shortcut: "Ctrl+T" });
    expect(find(strip, "reopen")).toMatchObject({ shortcut: "Ctrl+Shift+T" });
  });
  const tabActions = () => ({
    reload: vi.fn(),
    duplicate: vi.fn(),
    setFixed: vi.fn(),
    close: vi.fn(),
    split: vi.fn(),
    separate: vi.fn(),
  });

  it("pairs distinct unpaired tabs and separates a background pair by either member", () => {
    const tabs = [tab(1), tab(2), tab(3)];
    const acts = tabActions();
    select(find(tabMenu(tabs[1]!, tabs, acts, { active: 1, splits: [] }), "split"));
    expect(acts.split).toHaveBeenCalledWith(2);
    for (const member of [tabs[0]!, tabs[1]!]) {
      select(find(tabMenu(member, tabs, acts, { active: 3, splits: [{ tabs: [1, 2], ratio: 0.5 }] }), "separate"));
    }
    expect(acts.separate.mock.calls).toEqual([[1], [2]]);
    expect(acts.close).not.toHaveBeenCalled();
  });

  it("explains self-pairing, a paired target and a paired selection", () => {
    const tabs = [tab(1), tab(2), tab(3)];
    const reason = (target: number, active: number, splits: { tabs: [number, number]; ratio: number }[]) =>
      find(tabMenu(tabs[target - 1]!, tabs, tabActions(), { active, splits }), "split");
    expect(reason(1, 1, [])).toMatchObject({ disabled: true, disabledReason: "Select another tab to pair" });
    expect(reason(3, 1, [{ tabs: [1, 2], ratio: 0.5 }])).toMatchObject({
      disabled: true,
      disabledReason: "Separate the selected pair first",
    });
    expect(reason(1, 3, [{ tabs: [1, 2], ratio: 0.5 }])).toMatchObject({
      disabled: true,
      disabledReason: "Separate this pair first",
    });
  });

  it("closes the other tabs and the tabs to the right in strip order", () => {
    const tabs = [tab(1), tab(2), tab(3)];
    const acts = tabActions();
    const items = tabMenu(tabs[1]!, tabs, acts);
    select(find(items, "closeOthers"));
    select(find(items, "closeRight"));
    expect(acts.close).toHaveBeenNthCalledWith(1, [1, 3]);
    expect(acts.close).toHaveBeenNthCalledWith(2, [3]);
  });

  it("disables what has nothing to act on", () => {
    const only = tab(1, []);
    const items = tabMenu(only, [only], tabActions());
    expect(find(items, "closeOthers")).toMatchObject({ disabled: true });
    expect(find(items, "closeRight")).toMatchObject({ disabled: true });
    expect(find(items, "duplicate")).toMatchObject({ disabled: true });
  });

  it("toggles keeping the tab loaded", () => {
    const fixed = { ...tab(1), fixed: true };
    const acts = tabActions();
    const items = tabMenu(fixed, [fixed], acts);
    expect(find(items, "keep")).toMatchObject({ kind: "checkbox", checked: true });
    select(find(items, "keep"));
    expect(acts.setFixed).toHaveBeenCalledWith(1, false);
  });

  it("gives every item its own id, since the menu keys its rows by id", () => {
    const tabs = [tab(1), tab(2)];
    const items = tabMenu(tabs[0]!, tabs, tabActions());
    expect(new Set(ids(items)).size).toBe(items.length);
  });
});

describe("fieldMenu", () => {
  it("offers cut and copy only with a selection, and paste only when editable", () => {
    const acts = { cut: vi.fn(), copy: vi.fn(), paste: vi.fn(), selectAll: vi.fn() };
    const readOnly = fieldMenu(true, false, acts);
    expect(find(readOnly, "cut")).toMatchObject({ disabled: true });
    expect(find(readOnly, "copy")).toMatchObject({ disabled: false });
    expect(find(readOnly, "paste")).toMatchObject({ disabled: true });
    const empty = fieldMenu(false, true, acts);
    expect(find(empty, "copy")).toMatchObject({ disabled: true });
    expect(find(empty, "paste")).toMatchObject({ disabled: false });
  });
});

describe("interfaceMenu", () => {
  it("ends every interface menu with Inspect interface in development", () => {
    const acts = { cut: vi.fn(), copy: vi.fn(), paste: vi.fn(), selectAll: vi.fn() };
    for (const items of [fieldMenu(true, true, acts), stripMenu({ newTab: vi.fn(), reopen: vi.fn() })]) {
      expect(items.at(-1)).toMatchObject({ id: "inspectInterface" });
      expect(items.at(-2)).toMatchObject({ kind: "separator" });
    }
  });

  it("leaves the menu alone outside development", () => {
    vi.stubEnv("DEV", false);
    try {
      expect(interfaceMenu([]).length).toBe(0);
    } finally {
      vi.unstubAllEnvs();
    }
  });
});

describe("pageMenu", () => {
  it("offers opening a link privately after opening it in a new window", () => {
    const acts = actions();
    const items = pageMenu(request({ linkUrl: "https://link/" }), tab(1), acts);
    expect(ids(items).slice(ids(items).indexOf("linkWindow"), ids(items).indexOf("linkWindow") + 2)).toEqual([
      "linkWindow",
      "linkPrivateWindow",
    ]);
    select(find(items, "linkPrivateWindow"));
    expect(acts.openInPrivateWindow).toHaveBeenCalledExactlyOnceWith("https://link/");
  });

  it("omits the extra private-window action in a private window", () => {
    const acts = { ...actions(), isPrivate: true };
    expect(ids(pageMenu(request({ linkUrl: "https://link/" }), tab(1), acts))).not.toContain("linkPrivateWindow");
  });
  it("offers navigation on the page itself, from the tab's history", () => {
    const items = pageMenu(request({}), tab(1, ["a", "b"], 1), actions());
    expect(ids(items).slice(0, 6)).toEqual(["back", "forward", "reload", "print", "savePage", "separator:afterPage"]);
    expect(find(items, "back")).toMatchObject({ disabled: false, shortcut: "Alt+Left" });
    expect(find(items, "forward")).toMatchObject({ disabled: true, shortcut: "Alt+Right" });
    expect(find(items, "reload")).toMatchObject({ shortcut: "Ctrl+R" });
    expect(find(items, "inspect")).toMatchObject({ shortcut: "F12" });
  });

  it("prints and saves the page itself, with their chords", () => {
    const acts = actions();
    const items = pageMenu(request({}), tab(1), acts);
    expect(find(items, "print")).toMatchObject({ label: "Print\u2026", shortcut: "Ctrl+P" });
    expect(find(items, "savePage")).toMatchObject({ label: "Save as\u2026", shortcut: "Ctrl+S" });
    select(find(items, "print"));
    select(find(items, "savePage"));
    expect(acts.print).toHaveBeenCalledOnce();
    expect(acts.savePage).toHaveBeenCalledOnce();
    const linkItems = pageMenu(request({ linkUrl: "https://link/" }), tab(1), acts);
    expect(find(linkItems, "print")).toBeUndefined();
    expect(find(linkItems, "savePage")).toBeUndefined();
  });

  it("drops navigation over a link, and opens the link", () => {
    const acts = actions();
    const items = pageMenu(request({ linkUrl: "https://link/" }), tab(1), acts);
    expect(find(items, "back")).toBeUndefined();
    select(find(items, "linkWindow"));
    expect(acts.openInWindow).toHaveBeenCalledWith("https://link/");
  });

  it("drops navigation in an editable field", () => {
    expect(find(pageMenu(request({ editable: true }), tab(1), actions()), "back")).toBeUndefined();
  });

  it("opens an image in a tab", () => {
    const acts = actions();
    select(find(pageMenu(request({ kind: "image", sourceUrl: "https://img/" }), tab(1), acts), "imageTab"));
    expect(acts.openInTab).toHaveBeenCalledWith("https://img/");
  });

  it("quotes a long selection shortened, and searches for all of it", () => {
    const acts = actions();
    const text = "a ".repeat(40).trim();
    const search = find(pageMenu(request({ kind: "selection", selection: `  ${text} ` }), tab(1), acts), "search");
    expect(search && "label" in search && search.label).toContain("…");
    select(search);
    expect(acts.search).toHaveBeenCalledWith(text);
  });

  it("hands a native item back by its engine id", () => {
    const acts = actions();
    const items = pageMenu(request({ kind: "selection", selection: "x" }, [native(7, "copy")]), tab(1), acts);
    select(find(items, "native-7-copy"));
    expect(acts.choose).toHaveBeenCalledWith(7);
  });

  it("groups extension items at the bottom, above Inspect", () => {
    const items = pageMenu(
      request({ editable: true }, [native(1, "extension"), native(2, "paste")]),
      tab(1),
      actions(),
    );
    const order = items.filter((item) => item.kind !== "separator").map((item) => item.id);
    expect(order).toEqual(["native-2-paste", "native-1-extension", "inspect"]);
    expect(items.at(-2)?.kind).toBe("separator");
  });

  it("gives each native separator its own id", () => {
    const line = (id: number) => native(id, "other", { kind: "separator" });
    const items = pageMenu(
      request({ editable: true }, [native(1, "cut"), line(0), native(2, "paste"), line(0), native(3, "selectAll")]),
      tab(1),
      actions(),
    );
    const separators = items.filter((item) => item.kind === "separator").map((item) => item.id);
    expect(new Set(separators).size).toBe(separators.length);
  });

  it("shows a selection with replacement patterns literally", () => {
    const items = pageMenu(request({ kind: "selection", selection: "a $& b" }), tab(1), actions());
    expect(find(items, "search")).toMatchObject({ label: expect.stringContaining("a $& b") });
  });

  it("maps a native submenu with its children", () => {
    const sub = native(3, "spellCheck", { kind: "submenu", children: [native(4, "spellCheck")] });
    const item = find(pageMenu(request({ editable: true }, [sub]), tab(1), actions()), "native-3-spellCheck");
    expect(item).toMatchObject({ kind: "submenu", items: [{ id: "native-4-spellCheck" }] });
  });
});

describe("shortcut hints", () => {
  it("show the chord bound now, and none for an unbound shortcut", () => {
    keymapStore.set(keymap({ newTab: ["Alt+T", "Ctrl+T"], reopenClosedTab: [] }));
    const strip = stripMenu({ newTab: vi.fn(), reopen: vi.fn() });

    expect(find(strip, "new")).toMatchObject({ shortcut: "Alt+T" });
    expect(find(strip, "reopen")).toMatchObject({ shortcut: null });
  });
});
