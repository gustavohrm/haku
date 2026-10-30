import { applyTheme, isThemePreference } from "@app/theme";
import { BarSlot } from "@features/bars/bar-slot";
import { shellLayout } from "@features/bars/shell-layout";
import { ContextMenuHost } from "@features/context-menu/context-menu-host";
import { usePageMenu } from "@features/context-menu/use-page-menu";
import { usePageDialog } from "@features/feedback/use-page-dialog";
import { PopupBar } from "@features/popup/popup-bar";
import { TabStrip } from "@features/tabs/tab-strip";
import { Toolbar } from "@features/toolbar/toolbar";
import { ViewportPanes } from "@features/viewport/viewport-panes";
import { WindowControls } from "@features/window-controls/window-controls";
import { useActiveTab, useBrowserState, useSettings } from "@ipc/hooks";
import { useEffect } from "react";

export function App() {
  const { kind, private: isPrivate, tabs, active } = useBrowserState();
  const tab = useActiveTab();
  const settings = useSettings();
  usePageDialog(tab);
  usePageMenu(tab);

  const themePreference = settings?.theme;
  useEffect(() => {
    if (themePreference !== undefined && isThemePreference(themePreference)) {
      applyTheme(themePreference);
    }
  }, [themePreference]);

  const layout = shellLayout(settings, kind, isPrivate);
  const isFloating = layout.bars === "floating";
  // What a floating bar must show unasked: the toolbar names the page whose
  // dialog is open, and the tabs carry the badge of a dialog waiting elsewhere.
  const toolbarNeeded = tab !== null && tab.dialog !== null;
  const tabsNeeded = tabs.some((other) => other.id !== active && other.dialog !== null);

  return (
    // Compact is the default density; the attribute is where a density setting
    // will plug in. The layout attributes are read by the shell's grid.
    <div className="haku-shell" data-density="compact" data-tabs={layout.tabs} data-bars={layout.bars}>
      {/* A popup shows one page, so it has no tabs and no address to edit. */}
      {kind === "popup" ? (
        <BarSlot edge="top" contents={["toolbar"]} floating={false} isNeeded={false}>
          <PopupBar tab={tab} />
        </BarSlot>
      ) : layout.tabs === "horizontal" ? (
        <BarSlot edge="top" contents={["tabs", "toolbar"]} floating={isFloating} isNeeded={toolbarNeeded || tabsNeeded}>
          <header className="haku-bar flex h-8.5 shrink-0 items-center gap-2 pl-1.5" data-tauri-drag-region>
            <TabStrip />
            <WindowControls />
          </header>

          <Toolbar tab={tab} />
        </BarSlot>
      ) : (
        <>
          <BarSlot edge="top" contents={["toolbar"]} floating={isFloating} isNeeded={toolbarNeeded}>
            <Toolbar tab={tab} trailing={<WindowControls />} />
          </BarSlot>
          <BarSlot edge="side" contents={["tabs"]} floating={isFloating} isNeeded={tabsNeeded}>
            <div className="haku-bar flex min-h-0 flex-1 flex-col pt-px">
              <TabStrip layout="vertical" />
            </div>
          </BarSlot>
        </>
      )}

      <ViewportPanes />
      <ContextMenuHost />
    </div>
  );
}
