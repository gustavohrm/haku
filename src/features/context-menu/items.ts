/**
 * What each context menu holds, as plain item lists, kept apart from where the
 * menus open so they can be read and tested on their own.
 * [Context menus](../../../docs/specs/context-menus.md).
 */

import type { BrowserState, ContextMenuRequested, NativeMenuItem, Tab, TabId } from "@bindings";
import { reportUnexpected } from "@features/feedback/feedback";
import { tidySeparators, type MenuItem } from "@features/overlays/menu-model";
import { t } from "@shared/i18n";
import { invoke } from "@tauri-apps/api/core";

const separator = (id: string): MenuItem => ({ kind: "separator", id });

/** The longest selection a "Search for" label quotes in full. */
const QUOTED_LENGTH = 32;

/**
 * An interface menu's items, with **Inspect interface** at the end in
 * development builds, so working on Haku keeps the interface's DevTools.
 */
export function interfaceMenu(items: MenuItem[]): MenuItem[] {
  if (!import.meta.env.DEV) {
    return items;
  }
  return [
    ...items,
    separator("inspectInterface"),
    {
      kind: "command",
      id: "inspectInterface",
      label: t("menu.inspectInterface"),
      onSelect: () => void toggleInterfaceDevtools(),
    },
  ];
}

async function toggleInterfaceDevtools() {
  try {
    // Tauri's own command, which `core:default` grants the chrome.
    await invoke("plugin:webview|internal_toggle_devtools");
  } catch (error) {
    reportUnexpected(error);
  }
}

export interface TabActions {
  reload: (id: TabId) => void;
  duplicate: (url: string) => void;
  setFixed: (id: TabId, fixed: boolean) => void;
  close: (ids: TabId[]) => void;
  split: (id: TabId) => void;
  separate: (id: TabId) => void;
}

/**
 * A tab's menu.
 *
 * @param tab - The tab right-clicked.
 * @param tabs - Every tab in the window, in strip order.
 */
export function tabMenu(
  tab: Tab,
  tabs: readonly Tab[],
  actions: TabActions,
  { active, splits }: Pick<BrowserState, "active" | "splits"> = { active: null, splits: [] },
): MenuItem[] {
  const url = tab.history.entries[tab.history.index]?.url ?? null;
  const index = tabs.findIndex((other) => other.id === tab.id);
  const others = tabs.filter((other) => other.id !== tab.id).map((other) => other.id);
  const right = tabs.slice(index + 1).map((other) => other.id);
  const isPaired = splits.some((split) => split.tabs.includes(tab.id));
  const isSelectedPaired = active !== null && splits.some((split) => split.tabs.includes(active));
  const disabledReason = isSelectedPaired
    ? t("split.selectedPaired")
    : isPaired
      ? t("split.alreadyPaired")
      : active === null || active === tab.id
        ? t("split.self")
        : undefined;
  return interfaceMenu([
    {
      kind: "command",
      id: "reload",
      label: t("menu.reload"),
      onSelect: () => actions.reload(tab.id),
    },
    {
      kind: "command",
      id: "duplicate",
      label: t("menu.duplicate"),
      disabled: url === null,
      onSelect: () => url !== null && actions.duplicate(url),
    },
    {
      kind: "checkbox",
      id: "keep",
      label: t("menu.keepLoaded"),
      checked: tab.fixed,
      onSelect: () => actions.setFixed(tab.id, !tab.fixed),
    },
    separator("close"),
    {
      kind: "command",
      id: "split",
      label: t("split.open"),
      disabled: disabledReason !== undefined,
      disabledReason,
      onSelect: () => actions.split(tab.id),
    },
    ...(isPaired
      ? [
          {
            kind: "command" as const,
            id: "separate",
            label: t("split.separate"),
            onSelect: () => actions.separate(tab.id),
          },
        ]
      : []),
    { kind: "command", id: "close", label: t("menu.close"), onSelect: () => actions.close([tab.id]) },
    {
      kind: "command",
      id: "closeOthers",
      label: t("menu.closeOthers"),
      disabled: others.length === 0,
      onSelect: () => actions.close(others),
    },
    {
      kind: "command",
      id: "closeRight",
      label: t("menu.closeRight"),
      disabled: right.length === 0,
      onSelect: () => actions.close(right),
    },
  ]);
}

/** The tab strip's menu, away from any tab. */
export function stripMenu(actions: { newTab: () => void; reopen: () => void }): MenuItem[] {
  return interfaceMenu([
    { kind: "command", id: "new", label: t("tabs.new"), onSelect: actions.newTab },
    { kind: "command", id: "reopen", label: t("menu.reopenClosed"), onSelect: actions.reopen },
  ]);
}

export interface FieldActions {
  cut: () => void;
  copy: () => void;
  paste: () => void;
  selectAll: () => void;
}

/**
 * A text field's menu.
 *
 * @param selected - Whether the field has text selected.
 * @param editable - Whether the field can be typed in.
 */
export function fieldMenu(selected: boolean, editable: boolean, actions: FieldActions): MenuItem[] {
  return interfaceMenu([
    { kind: "command", id: "cut", label: t("menu.cut"), disabled: !selected || !editable, onSelect: actions.cut },
    {
      kind: "command",
      id: "copy",
      label: t("menu.copy"),
      disabled: !selected,
      onSelect: actions.copy,
    },
    { kind: "command", id: "paste", label: t("menu.paste"), disabled: !editable, onSelect: actions.paste },
    separator("all"),
    { kind: "command", id: "selectAll", label: t("menu.selectAll"), onSelect: actions.selectAll },
  ]);
}

export interface PageActions {
  back: () => void;
  forward: () => void;
  reload: () => void;
  inspect: () => void;
  openInTab: (url: string) => void;
  openInWindow: (url: string) => void;
  search: (text: string) => void;
  /** A native item was chosen, by its engine id. */
  choose: (id: number) => void;
}

/**
 * A page's menu: Haku's own items for what was clicked, then the engine's,
 * with items extensions added in a group of their own at the bottom.
 *
 * @param tab - The tab whose page was right-clicked.
 */
export function pageMenu(request: ContextMenuRequested, tab: Tab | null, actions: PageActions): MenuItem[] {
  const { target } = request;
  const items: MenuItem[] = [];

  if (target.linkUrl !== null) {
    const link = target.linkUrl;
    items.push(
      { kind: "command", id: "linkTab", label: t("menu.openLinkTab"), onSelect: () => actions.openInTab(link) },
      {
        kind: "command",
        id: "linkWindow",
        label: t("menu.openLinkWindow"),
        onSelect: () => actions.openInWindow(link),
      },
      separator("afterLink"),
    );
  }
  if (target.kind === "image" && target.sourceUrl !== null) {
    const image = target.sourceUrl;
    items.push(
      { kind: "command", id: "imageTab", label: t("menu.openImageTab"), onSelect: () => actions.openInTab(image) },
      separator("afterImage"),
    );
  }
  const selection = target.selection?.trim() ?? "";
  if (selection !== "") {
    items.push(
      {
        kind: "command",
        id: "search",
        // A function, so `$&` and the like in the selection stay literal.
        label: t("menu.searchFor").replace("{text}", () => quote(selection)),
        onSelect: () => actions.search(selection),
      },
      separator("afterSearch"),
    );
  }
  // The page itself, as opposed to something on it.
  if (target.kind === "page" && target.linkUrl === null && !target.editable) {
    const history = tab?.history;
    items.push(
      {
        kind: "command",
        id: "back",
        label: t("toolbar.back"),
        disabled: !history || history.index <= 0,
        onSelect: actions.back,
      },
      {
        kind: "command",
        id: "forward",
        label: t("toolbar.forward"),
        disabled: !history || history.index >= history.entries.length - 1,
        onSelect: actions.forward,
      },
      { kind: "command", id: "reload", label: t("toolbar.reload"), onSelect: actions.reload },
      separator("afterPage"),
    );
  }

  const extension = (item: NativeMenuItem) => item.name === "extension";
  const native = (item: NativeMenuItem, index: number) => nativeItem(item, actions.choose, `${index}`);
  items.push(...request.items.filter((item) => !extension(item)).map(native));
  items.push(separator("extensions"));
  items.push(...request.items.filter(extension).map((item, index) => native(item, -1 - index)));
  items.push(separator("inspect"));
  items.push({ kind: "command", id: "inspect", label: t("menu.inspect"), onSelect: actions.inspect });

  return tidySeparators(items);
}

function quote(text: string): string {
  const flat = text.replace(/\s+/g, " ");
  return flat.length > QUOTED_LENGTH ? `${flat.slice(0, QUOTED_LENGTH - 1)}…` : flat;
}

/**
 * One of the engine's items, which runs by being handed back to it.
 *
 * @param place - Where it sits in the engine's menu, which keeps separators
 * apart: they share a name, and need not have ids of their own.
 */
function nativeItem(item: NativeMenuItem, choose: (id: number) => void, place: string): MenuItem {
  const id = `native-${item.id}-${item.name}`;
  const common = {
    id,
    label: item.label,
    disabled: !item.enabled,
    shortcut: item.shortcut,
  };
  switch (item.kind) {
    case "separator":
      return { kind: "separator", id: `native-separator-${place}` };
    case "submenu":
      return {
        ...common,
        kind: "submenu",
        items: tidySeparators(item.children.map((child, index) => nativeItem(child, choose, `${place}.${index}`))),
      };
    case "checkbox":
    case "radio":
      return { ...common, kind: item.kind, checked: item.checked, onSelect: () => choose(item.id) };
    case "command":
      return { ...common, kind: "command", onSelect: () => choose(item.id) };
  }
}
