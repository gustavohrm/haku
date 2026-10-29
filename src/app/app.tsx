import { applyTheme, isThemePreference } from "@app/theme";
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
  const { kind } = useBrowserState();
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

  return (
    // Compact is the default density; the attribute is where a density setting
    // will plug in.
    <div className="flex h-full flex-col" data-density="compact">
      {/* A popup shows one page, so it has no tabs and no address to edit. */}
      {kind === "popup" ? (
        <PopupBar tab={tab} />
      ) : (
        <>
          <header className="haku-bar flex h-8.5 shrink-0 items-center gap-2 pl-1.5" data-tauri-drag-region>
            <TabStrip />
            <WindowControls />
          </header>

          <Toolbar tab={tab} />
        </>
      )}

      <ViewportPanes />
      <ContextMenuHost />
    </div>
  );
}
