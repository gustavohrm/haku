import { SETTINGS_URL } from "@app/router";
import { t } from "@shared/i18n";

import {
  AddExtensionControl,
  AddressBarLayoutControl,
  BarLayoutControl,
  BookmarksBarControl,
  CapacityControl,
  DiscardControl,
  ExtensionsControl,
  ExtensionsFolderControl,
  FreezeControl,
  HistoryPageControl,
  HomeUrlControl,
  KeptMemoryControl,
  KeptSitesControl,
  MemoryPageControl,
  PresetControl,
  SidebarBookmarksControl,
  SidebarSideControl,
  SitePermissionsControl,
  SiteZoomControl,
  TabLayoutControl,
  ThemeControl,
  ToolbarPlacementControl,
  VersionControl,
} from "./controls";
import { HistoryEnabledControl, HistoryRetentionControl, HistoryRulesControl } from "./history-controls";
import { SearchEngineControl, SearchEnginesControl, SearchSuggestionsControl } from "./search-controls";
import { SHORTCUT_GROUPS } from "./shortcuts";
import { type SettingRowDefinition, type SettingSection } from "./types";

/*
  Every setting, as data. The sidebar and search both read this list, so they
  cannot disagree about what exists, and a new setting is a row here, never a
  new layout.
*/

// These settings take effect only while the tabs are vertical.
const unlessVerticalTabs = (settings: { tabLayout: string }) =>
  settings.tabLayout === "vertical" ? null : ("settings.needsVerticalTabs" as const);

// Where bookmarks go is a choice only while the toolbar is in the column.
const unlessToolbarInColumn = (settings: { tabLayout: string; toolbarPlacement: string }) =>
  settings.tabLayout === "vertical" && settings.toolbarPlacement === "sidebar"
    ? null
    : ("settings.needsToolbarInColumn" as const);

// The address field in the column always floats.
const ifToolbarInColumn = (settings: { tabLayout: string; toolbarPlacement: string }) =>
  unlessToolbarInColumn(settings) === null ? ("settings.disabledByToolbarInColumn" as const) : null;

// The bar shows nowhere while bookmarks are the column's button.
const unlessBookmarksBarShown = (settings: {
  tabLayout: string;
  toolbarPlacement: string;
  sidebarBookmarks: string;
}) =>
  unlessToolbarInColumn(settings) === null && settings.sidebarBookmarks === "button"
    ? ("settings.disabledByBookmarksButton" as const)
    : null;

// Discarding every background tab leaves nothing to freeze, nothing for extra
// slots to hold and nothing to spend a budget on.
const unlessDiscardingEverything = (settings: { discardTabs: string }) =>
  settings.discardTabs === "always" ? ("settings.disabledByDiscard" as const) : null;

export const SECTIONS: SettingSection[] = [
  {
    id: "appearance",
    icon: "ic-palette",
    title: "settings.appearance",
    groups: [
      {
        rows: [
          {
            id: "theme",
            title: "settings.theme",
            description: "settings.theme.help",
            keywords: "settings.theme.keywords",
            labelsControl: false,
            Control: ThemeControl,
          },
          {
            id: "tabLayout",
            title: "settings.tabLayout",
            description: "settings.tabLayout.help",
            keywords: "settings.tabLayout.keywords",
            labelsControl: false,
            Control: TabLayoutControl,
          },
          {
            id: "sidebarSide",
            title: "settings.sidebarSide",
            description: "settings.sidebarSide.help",
            keywords: "settings.sidebarSide.keywords",
            labelsControl: false,
            disabledReason: unlessVerticalTabs,
            Control: SidebarSideControl,
          },
          {
            id: "toolbarPlacement",
            title: "settings.toolbarPlacement",
            description: "settings.toolbarPlacement.help",
            keywords: "settings.toolbarPlacement.keywords",
            labelsControl: false,
            disabledReason: unlessVerticalTabs,
            Control: ToolbarPlacementControl,
          },
          {
            id: "addressBarLayout",
            title: "settings.addressBarLayout",
            description: "settings.addressBarLayout.help",
            keywords: "settings.addressBarLayout.keywords",
            labelsControl: false,
            disabledReason: ifToolbarInColumn,
            Control: AddressBarLayoutControl,
          },
          {
            id: "sidebarBookmarks",
            title: "settings.sidebarBookmarks",
            description: "settings.sidebarBookmarks.help",
            keywords: "settings.sidebarBookmarks.keywords",
            labelsControl: false,
            disabledReason: unlessToolbarInColumn,
            Control: SidebarBookmarksControl,
          },
          {
            id: "bookmarksBar",
            title: "settings.bookmarksBar",
            description: "settings.bookmarksBar.help",
            keywords: "settings.bookmarksBar.keywords",
            disabledReason: unlessBookmarksBarShown,
            Control: BookmarksBarControl,
          },
          {
            id: "barLayout",
            title: "settings.barLayout",
            description: "settings.barLayout.help",
            keywords: "settings.barLayout.keywords",
            labelsControl: false,
            Control: BarLayoutControl,
          },
        ],
      },
    ],
  },
  {
    id: "tabs",
    icon: "ic-gauge",
    title: "settings.section.tabs",
    groups: [
      {
        title: "settings.optimization",
        rows: [
          {
            id: "preset",
            title: "settings.preset",
            description: "settings.preset.help",
            Control: PresetControl,
          },
          {
            id: "capacity",
            title: "settings.capacity",
            description: "settings.capacity.help",
            keywords: "settings.capacity.keywords",
            disabledReason: unlessDiscardingEverything,
            Control: CapacityControl,
          },
          {
            id: "keptMemory",
            title: "settings.keptMemory",
            description: "settings.keptMemory.help",
            keywords: "settings.keptMemory.keywords",
            disabledReason: unlessDiscardingEverything,
            Control: KeptMemoryControl,
          },
          {
            id: "freeze",
            title: "settings.freeze",
            description: "settings.freeze.help",
            keywords: "settings.freeze.keywords",
            disabledReason: unlessDiscardingEverything,
            Control: FreezeControl,
          },
          {
            id: "discard",
            title: "settings.discard",
            description: "settings.discard.help",
            keywords: "settings.discard.keywords",
            Control: DiscardControl,
          },
        ],
      },
      {
        rows: [
          {
            id: "keptSites",
            title: "settings.keptSites",
            description: "settings.keptSites.help",
            keywords: "settings.keptSites.keywords",
            wide: true,
            labelsControl: false,
            Control: KeptSitesControl,
          },
        ],
      },
    ],
  },
  {
    id: "browsing",
    icon: "ic-globe",
    title: "settings.browsing",
    groups: [
      {
        rows: [
          {
            id: "searchEngine",
            title: "settings.searchEngine",
            description: "settings.searchEngine.help",
            keywords: "settings.searchEngine.keywords",
            Control: SearchEngineControl,
          },
          {
            id: "searchEngines",
            title: "settings.searchEngines",
            description: "settings.searchEngines.help",
            keywords: "settings.searchEngines.keywords",
            wide: true,
            labelsControl: false,
            Control: SearchEnginesControl,
          },
          {
            id: "searchSuggestions",
            title: "settings.searchSuggestions",
            description: "settings.searchSuggestions.help",
            keywords: "settings.searchSuggestions.keywords",
            Control: SearchSuggestionsControl,
          },
          {
            id: "homeUrl",
            title: "settings.home",
            description: "settings.home.help",
            keywords: "settings.home.keywords",
            wide: true,
            Control: HomeUrlControl,
          },
        ],
      },
      {
        rows: [
          {
            id: "sitePermissions",
            title: "settings.sitePermissions",
            description: "settings.sitePermissions.help",
            keywords: "settings.sitePermissions.keywords",
            wide: true,
            labelsControl: false,
            Control: SitePermissionsControl,
          },
          {
            id: "siteZoom",
            title: "settings.siteZoom",
            description: "settings.siteZoom.help",
            keywords: "settings.siteZoom.keywords",
            wide: true,
            labelsControl: false,
            Control: SiteZoomControl,
          },
        ],
      },
      {
        title: "settings.history",
        rows: [
          {
            id: "historyEnabled",
            title: "settings.historyEnabled",
            description: "settings.historyEnabled.help",
            keywords: "settings.historyEnabled.keywords",
            Control: HistoryEnabledControl,
          },
          {
            id: "historyRetention",
            title: "settings.historyRetention",
            description: "settings.historyRetention.help",
            keywords: "settings.historyRetention.keywords",
            Control: HistoryRetentionControl,
          },
          {
            id: "historyRules",
            title: "settings.historyRules",
            description: "settings.historyRules.help",
            keywords: "settings.historyRules.keywords",
            wide: true,
            labelsControl: false,
            Control: HistoryRulesControl,
          },
        ],
      },
    ],
  },
  {
    id: "extensions",
    icon: "ic-puzzle",
    title: "settings.extensions",
    groups: [
      {
        rows: [
          {
            id: "addExtension",
            title: "settings.extensions.add",
            description: "settings.extensions.add.help",
            keywords: "settings.extensions.add.keywords",
            labelsControl: false,
            Control: AddExtensionControl,
          },
          {
            id: "installed",
            title: "settings.extensions.installed",
            keywords: "settings.extensions.keywords",
            wide: true,
            labelsControl: false,
            Control: ExtensionsControl,
          },
          {
            id: "extensionsFolder",
            title: "settings.extensions.folderTitle",
            description: "settings.extensions.folder.help",
            keywords: "settings.extensions.folder.keywords",
            Control: ExtensionsFolderControl,
          },
        ],
      },
    ],
  },
  {
    id: "shortcuts",
    icon: "ic-keyboard",
    title: "settings.section.shortcuts",
    groups: SHORTCUT_GROUPS,
  },
  {
    id: "about",
    icon: "ic-info",
    title: "settings.section.about",
    groups: [
      {
        rows: [
          {
            id: "version",
            title: "settings.version",
            keywords: "settings.version.keywords",
            labelsControl: false,
            Control: VersionControl,
          },
          {
            id: "memoryPage",
            title: "memory.title",
            description: "settings.memoryPage.help",
            Control: MemoryPageControl,
          },
          {
            id: "historyPage",
            title: "history.title",
            description: "settings.historyPage.help",
            Control: HistoryPageControl,
          },
        ],
      },
    ],
  },
];

/** The section shown at `haku://settings` itself, and for an unknown section. */
export const DEFAULT_SECTION = SECTIONS[0] as SettingSection;

/**
 * The section a settings URL points at.
 *
 * @param url - A `haku://settings` URL, such as `haku://settings/tabs`.
 * @returns The section named by its first path segment, or the default one.
 */
export function sectionOf(url: string): SettingSection {
  const rest = url.startsWith(SETTINGS_URL) ? url.slice(SETTINGS_URL.length) : "";
  const id = rest.replace(/^\//, "").split(/[/?#]/, 1)[0] ?? "";
  return SECTIONS.find((section) => section.id === id) ?? DEFAULT_SECTION;
}

/**
 * The row a settings URL points at, by the id after its `#`, such as
 * `haku://settings/browsing#historyEnabled`.
 *
 * @param url - A `haku://settings` URL.
 * @returns The row's id, or `null` when the URL names none.
 */
export function rowOf(url: string): string | null {
  const hash = url.indexOf("#");
  const id = hash === -1 ? "" : url.slice(hash + 1);
  return id === "" ? null : id;
}

/** Where a section lives. */
export function sectionUrl(section: SettingSection): string {
  return `${SETTINGS_URL}/${section.id}`;
}

export interface SearchResult {
  section: SettingSection;
  rows: SettingRowDefinition[];
}

/**
 * The rows matching a search, grouped by section, in the order they appear.
 *
 * A row matches when every word of the query appears in its translated title,
 * description or keywords, so "dark" finds Theme and "background memory"
 * finds the memory budget. Sections without a match are left out.
 *
 * @param query - What was typed into the search box.
 */
export function searchSettings(query: string): SearchResult[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return [];
  }
  return SECTIONS.map((section) => ({
    section,
    rows: section.groups
      .flatMap((group) => group.rows)
      .filter((row) => {
        const text = [row.title, row.description, row.keywords]
          .map((key) => (key ? t(key) : ""))
          .join(" ")
          .toLowerCase();
        return words.every((word) => text.includes(word));
      }),
  })).filter((result) => result.rows.length > 0);
}
