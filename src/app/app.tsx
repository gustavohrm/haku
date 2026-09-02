import { renderInternal } from "@app/router";
import { applyTheme, isThemePreference } from "@app/theme";
import { commands } from "@bindings";
import { TabStrip } from "@features/tabs/tab-strip";
import { Toolbar } from "@features/toolbar/toolbar";
import { useChromeLayout } from "@features/viewport/use-chrome-layout";
import { WindowControls } from "@features/window-controls/window-controls";
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
    <div className="haku-shell">
      <header className="haku-titlebar" data-tauri-drag-region>
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
      <main ref={viewportRef} className="haku-viewport">
        {internalPage}
      </main>
    </div>
  );
}
