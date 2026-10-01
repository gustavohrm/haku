import { renderInternal } from "@app/router";
import { applyTheme, isThemePreference } from "@app/theme";
import { usePageDialog } from "@features/feedback/use-page-dialog";
import { TabStrip } from "@features/tabs/tab-strip";
import { Toolbar } from "@features/toolbar/toolbar";
import { useChromeLayout } from "@features/viewport/use-chrome-layout";
import { WindowControls } from "@features/window-controls/window-controls";
import { commands } from "@ipc/commands";
import { useActiveTab, useSettings } from "@ipc/hooks";
import { useEffect, useRef } from "react";

/** How often to check whether a pinned tab has gone idle. */
const IDLE_SWEEP_MS = 30_000;

export function App() {
  const tab = useActiveTab();
  const settings = useSettings();
  const viewportRef = useRef<HTMLDivElement>(null);

  const internal = tab?.presence.status === "internal";
  const activeUrl = tab?.history.entries[tab.history.index]?.url ?? "";
  const internalPage = internal ? renderInternal(activeUrl) : null;

  // An internal page is drawn by the chrome itself, so nothing should show
  // through and the chrome stays solid over the whole surface.
  useChromeLayout(viewportRef, internal);
  usePageDialog(tab);

  const themePreference = settings?.theme;
  useEffect(() => {
    if (themePreference !== undefined && isThemePreference(themePreference)) {
      applyTheme(themePreference);
    }
  }, [themePreference]);

  useEffect(() => {
    const timer = setInterval(() => void commands.releaseIdleTabs(), IDLE_SWEEP_MS);
    return () => clearInterval(timer);
  }, []);

  return (
    // Compact is the default density; the attribute is where a density setting
    // will plug in.
    <div className="flex h-full flex-col" data-density="compact">
      <header className="haku-bar flex h-8.5 shrink-0 items-center gap-2 pl-1.5" data-tauri-drag-region>
        <TabStrip />
        <WindowControls />
      </header>

      <Toolbar tab={tab} />

      {/*
        The page surface. When a web page is showing this is an empty element:
        Rust cuts a hole in the chrome's input mask exactly here so the content
        webview underneath receives clicks. Its rounded frame is drawn by the
        chrome on top, which is why the corners look right over any page.
      */}
      <main ref={viewportRef} className="haku-viewport relative mx-1 mb-1 min-h-0 flex-1 overflow-hidden">
        {internalPage}
      </main>
    </div>
  );
}
