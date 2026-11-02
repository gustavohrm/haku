import { applyTheme, isThemePreference } from "@app/theme";
import { BarSlot } from "@features/bars/bar-slot";
import { shellLayout } from "@features/bars/shell-layout";
import { SidebarResizer } from "@features/bars/sidebar-resizer";
import { ContextMenuHost } from "@features/context-menu/context-menu-host";
import { usePageMenu } from "@features/context-menu/use-page-menu";
import { useCloseConfirmation } from "@features/downloads/use-close-confirmation";
import { isWaiting } from "@features/feedback/page-dialog";
import { usePageDialog } from "@features/feedback/use-page-dialog";
import { usePermissionPrompt } from "@features/feedback/use-permission-prompt";
import { useFullscreenNotice } from "@features/fullscreen/use-fullscreen-notice";
import { PopupBar } from "@features/popup/popup-bar";
import { TabStrip } from "@features/tabs/tab-strip";
import { SidebarActions, SidebarToolbar, Toolbar } from "@features/toolbar/toolbar";
import { ViewportPanes } from "@features/viewport/viewport-panes";
import { WindowControls } from "@features/window-controls/window-controls";
import { useActiveTab, useBrowserState, useSettings } from "@ipc/hooks";
import { useEffect, type CSSProperties, type ReactNode } from "react";

export function App() {
  const { kind, private: isPrivate, tabs, active, fullscreen } = useBrowserState();
  const tab = useActiveTab();
  const settings = useSettings();
  usePageDialog(tab);
  usePermissionPrompt(tab, isPrivate);
  usePageMenu(tab);
  useCloseConfirmation();
  const fullscreenTab = tabs.find((candidate) => candidate.id === fullscreen);
  useFullscreenNotice(
    fullscreenTab?.id ?? null,
    fullscreenTab?.history.entries[fullscreenTab.history.index]?.url ?? "",
  );

  const themePreference = settings?.theme;
  useEffect(() => {
    if (themePreference !== undefined && isThemePreference(themePreference)) {
      applyTheme(themePreference);
    }
  }, [themePreference]);

  const layout = shellLayout(settings, kind, isPrivate);
  const isFloating = layout.bars === "floating";
  // What a floating bar must show unasked: the toolbar names the page whose
  // dialog or permission request is open, and the tabs carry the badge of one
  // waiting elsewhere.
  const toolbarNeeded = tab !== null && isWaiting(tab);
  const tabsNeeded = tabs.some((other) => other.id !== active && isWaiting(other));

  // The column, docked or floating, and the edge that resizes it.
  const sidebar = (contents: ReactNode) => (
    <div className="haku-bar haku-sidebar" data-tauri-drag-region>
      {contents}
      <SidebarResizer side={layout.side} width={layout.sidebarWidth} />
    </div>
  );

  return (
    // Compact is the default density; the attribute is where a density setting
    // will plug in. The layout attributes are read by the shell's grid, and
    // the sidebar's width by its column, floating or docked.
    <div
      className="haku-shell"
      data-density="compact"
      // A page in fullscreen hides every bar and fills the screen; the bars
      // stay mounted, so typed text and scroll survive. The layout attributes
      // are dropped meanwhile, so no layout's grid or margins outrank it.
      data-fullscreen={fullscreenTab ? "" : undefined}
      data-tabs={fullscreenTab ? undefined : layout.tabs}
      data-bars={fullscreenTab ? undefined : layout.bars}
      data-side={fullscreenTab ? undefined : layout.side}
      data-toolbar={fullscreenTab ? undefined : layout.toolbar}
      style={{ "--haku-sidebar-width": `${layout.sidebarWidth}px` } as CSSProperties}
    >
      {/* A popup shows one page, so it has no tabs and no address to edit. */}
      {kind === "popup" ? (
        <BarSlot edge="top" contents={["toolbar"]} floating={false} isNeeded={false}>
          <PopupBar tab={tab} />
        </BarSlot>
      ) : layout.tabs === "horizontal" ? (
        <BarSlot edge="top" contents={["tabs", "toolbar"]} floating={isFloating} isNeeded={toolbarNeeded || tabsNeeded}>
          <header
            className="haku-bar flex h-[calc(var(--control-height)+var(--haku-gap))] shrink-0 items-end gap-(--haku-gap) pl-(--haku-gap)"
            data-tauri-drag-region
          >
            <TabStrip />
            <WindowControls />
          </header>

          <Toolbar tab={tab} />
        </BarSlot>
      ) : layout.toolbar === "sidebar" ? (
        // One bar: the toolbar heads the column, which runs the window's height.
        // No window controls: its free space moves and maximizes the window.
        <BarSlot
          edge="side"
          contents={["tabs", "toolbar"]}
          floating={isFloating}
          isNeeded={toolbarNeeded || tabsNeeded}
        >
          {sidebar(
            <>
              <SidebarToolbar tab={tab} />
              <TabStrip layout="vertical" />
              <SidebarActions tab={tab} />
            </>,
          )}
        </BarSlot>
      ) : (
        <>
          <BarSlot edge="top" contents={["toolbar"]} floating={isFloating} isNeeded={toolbarNeeded}>
            <Toolbar tab={tab} trailing={<WindowControls />} />
          </BarSlot>
          <BarSlot edge="side" contents={["tabs"]} floating={isFloating} isNeeded={tabsNeeded}>
            {sidebar(<TabStrip layout="vertical" />)}
          </BarSlot>
        </>
      )}

      <ViewportPanes />
      <ContextMenuHost />
    </div>
  );
}
